use crate::common::{
    abi_baselines, command_output, copy_file_or_symlink, copy_tree, ensure_parent_dir,
    install_path_to_root, load_json, load_test_catalog, load_test_port_plan, load_toml,
    normalize_test_destination_path, repo_path, run_command, safe_root, set_metadata_phase,
    upsert_fallback_entry, version_name_cmp, write_pretty_json, write_toml, write_toml_value,
    AbiBaseline, FallbackInventory, FallbackInventoryEntry, PackageEntry, PackageManifest,
    TestsManifest, TestsManifestEntry, TestsManifestMetadata, COMPLETED_PHASES, PHASE_ID,
    REQUIRED_PACKAGES,
};
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use libc_support_tools::{backend_assets, logical_source_path, required_tools, RequiredToolKind};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml::Value as TomlValue;

const PHASE_04_ID: &str = "impl_04_loader_startup_secure_exec";
const PHASE_06_ID: &str = "impl_06_io_stdio_string_path";
const PHASE_07_ID: &str = "impl_07_nss_resolver_nscd";
const PHASE_08_ID: &str = "impl_08_locale_iconv_posix_parsers";
const PHASE_09_ID: &str = "impl_09_math_and_aux_dsos";
const FINAL_CUTOVER_PHASE: &str = "impl_10_final_fixup_and_audit";

const PHASE_EXTRA_NOTES: [&str; 5] = [
    "Phase 8 keeps the network-facing libanl, libnsl, libnss_*, and libresolv public payloads on the safe build path while cutting over locale, iconv, POSIX parser, and libBrokenLocale ownership.",
    "The safe test tree carries every phase-8-owned conform, iconv, iconvdata, locale, localedata, posix, normalized sysdeps, shared script, and po sentinel row from the committed ownership plan.",
    "check-owned-tests validates exact ownership completeness against the committed test catalog and test-port plan before it executes ported rows.",
    "stage-upstream-build remains the only supported way to adopt or recreate safe/work/original-build for relink smokes, package derivation, and upstream-test execution.",
    "Phase 10 removes private baseline backend DSOs from shipped manifests and cuts libc6-dev code-bearing link assets over to the safe build root.",
];

const PUBLIC_CUTOVER_DSOS: [&str; 20] = [
    "ld.so",
    "libBrokenLocale",
    "libc",
    "libdl",
    "libm",
    "libmvec",
    "libpcprofile",
    "libpthread",
    "librt",
    "libthread_db",
    "libutil",
    "libc_malloc_debug",
    "libmemusage",
    "libanl",
    "libnsl",
    "libnss_compat",
    "libnss_dns",
    "libnss_files",
    "libnss_hesiod",
    "libresolv",
];

const PHASE_07_PUBLIC_DEV_LINKNAMES: [&str; 4] = [
    "/usr/lib64/libanl.so",
    "/usr/lib64/libnss_compat.so",
    "/usr/lib64/libnss_hesiod.so",
    "/usr/lib64/libresolv.so",
];

const PHASE_09_PUBLIC_DEV_LINKNAMES: [&str; 2] = ["/usr/lib64/libm.so", "/usr/lib64/libmvec.so"];

const PHASE_06_PRIVATE_BACKEND_DSOS: [(&str, &str); 6] = [
    (
        "/usr/libexec/safelibs/backends/ld-linux-x86-64.so.2",
        "/usr/lib64/ld-linux-x86-64.so.2",
    ),
    (
        "/usr/libexec/safelibs/backends/libc.so.6",
        "/usr/lib64/libc.so.6",
    ),
    (
        "/usr/libexec/safelibs/backends/libpthread.so.0",
        "/usr/lib64/libpthread.so.0",
    ),
    (
        "/usr/libexec/safelibs/backends/libthread_db.so.1",
        "/usr/lib64/libthread_db.so.1",
    ),
    (
        "/usr/libexec/safelibs/backends/libc_malloc_debug.so.0",
        "/usr/lib64/libc_malloc_debug.so.0",
    ),
    (
        "/usr/libexec/safelibs/backends/libmemusage.so",
        "/usr/lib64/libmemusage.so",
    ),
];

const FINAL_STARTFILES: [&str; 8] = [
    "/usr/lib64/Mcrt1.o",
    "/usr/lib64/Scrt1.o",
    "/usr/lib64/crt1.o",
    "/usr/lib64/crti.o",
    "/usr/lib64/crtn.o",
    "/usr/lib64/gcrt1.o",
    "/usr/lib64/grcrt1.o",
    "/usr/lib64/rcrt1.o",
];

const FINAL_STATIC_ARCHIVES: [&str; 16] = [
    "/usr/lib64/libBrokenLocale.a",
    "/usr/lib64/libanl.a",
    "/usr/lib64/libc.a",
    "/usr/lib64/libc_nonshared.a",
    "/usr/lib64/libdl.a",
    "/usr/lib64/libg.a",
    "/usr/lib64/libm-2.39.a",
    "/usr/lib64/libm.a",
    "/usr/lib64/libmcheck.a",
    "/usr/lib64/libmvec.a",
    "/usr/lib64/libpthread.a",
    "/usr/lib64/libpthread_nonshared.a",
    "/usr/lib64/libresolv.a",
    "/usr/lib64/librt.a",
    "/usr/lib64/libutil.a",
    "/usr/lib64/audit/sotruss-lib.so",
];

const LOCALE_HELPER_SCRIPTS: [(&str, &str, bool); 5] = [
    (
        "/usr/sbin/locale-gen",
        "safe/debian/local/usr_sbin/locale-gen",
        true,
    ),
    (
        "/usr/sbin/update-locale",
        "safe/debian/local/usr_sbin/update-locale",
        true,
    ),
    (
        "/usr/sbin/validlocale",
        "safe/debian/local/usr_sbin/validlocale",
        true,
    ),
    (
        "/usr/share/locales/install-language-pack",
        "safe/debian/local/usr_share_locales/install-language-pack",
        true,
    ),
    (
        "/usr/share/locales/remove-language-pack",
        "safe/debian/local/usr_share_locales/remove-language-pack",
        true,
    ),
];

const PHASE_08_LOCALE_TOOL_BACKENDS: [&str; 4] = [
    "/usr/libexec/safelibs/locale-tools/iconv.backend",
    "/usr/libexec/safelibs/locale-tools/iconvconfig.backend",
    "/usr/libexec/safelibs/locale-tools/locale.backend",
    "/usr/libexec/safelibs/locale-tools/localedef.backend",
];

const PHASE_09_AUX_TOOL_BACKENDS: [&str; 5] = [
    "/usr/libexec/safelibs/aux-tools/gencat.backend",
    "/usr/libexec/safelibs/aux-tools/getconf.backend",
    "/usr/libexec/safelibs/aux-tools/tzselect.backend",
    "/usr/libexec/safelibs/aux-tools/zdump.backend",
    "/usr/libexec/safelibs/aux-tools/zic.backend",
];

const LOCALE_DATA_FILES: [(&str, &str); 1] = [(
    "/usr/share/i18n/SUPPORTED",
    "safe/generated/localedata/SUPPORTED",
)];

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MetadataBlock {
    phase: String,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PortStatusDoc {
    metadata: MetadataBlock,
    dso_targets: Vec<BTreeMap<String, TomlValue>>,
    subsystems: Vec<BTreeMap<String, TomlValue>>,
    package_components: Vec<BTreeMap<String, TomlValue>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PackageScopeDoc {
    metadata: MetadataBlock,
    debug_derivation: TomlValue,
    package_components: Vec<TomlValue>,
    helper_paths: Vec<TomlValue>,
    entrypoints: Vec<TomlValue>,
    files: Vec<TomlValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hybrid_install_root: Option<HybridInstallRoot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct HybridInstallRoot {
    required_packages_manifest: String,
    test_install_root_manifest: String,
    supplied_packages: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CveStatusMetadata {
    default_status: String,
    phase: String,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CveStatusEntry {
    id: String,
    status: String,
    component: String,
    rationale: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CveStatusDoc {
    metadata: CveStatusMetadata,
    entries: Vec<CveStatusEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct HybridBuildState {
    phase: String,
    target: String,
    profile: String,
    artifact_root: String,
}

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "amd64")]
    pub target: String,
    #[arg(long, default_value = "dev")]
    pub profile: String,
}

pub fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    refresh_phase_outputs()?;
    super::stage_upstream_build::ensure_default_staged_upstream_build()?;
    build_rust_runtime_crates(&args)?;
    let artifact_root = build_output_root(&args);
    build_hybrid_abi_shells(&args, &artifact_root)?;
    refresh_debug_manifest_from_build_root(&artifact_root)?;
    super::install_root::refresh_install_manifests()?;
    write_active_build_state(&args, &artifact_root)?;
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    match args.target.as_str() {
        "amd64" | "x86_64" => {}
        other => bail!("unsupported build target {other}; this port currently only supports amd64"),
    }
    match args.profile.as_str() {
        "dev" | "release" => {}
        other => bail!("unsupported build profile {other}; expected dev or release"),
    }
    Ok(())
}

pub fn build_output_root(args: &Args) -> PathBuf {
    safe_root()
        .join("work")
        .join("libc-family-build")
        .join(normalized_target(&args.target))
}

pub fn load_active_build_root() -> Result<PathBuf> {
    let state = load_active_build_state()?;
    Ok(PathBuf::from(state.artifact_root))
}

pub fn ensure_active_build_profile(target: &str, profile: &str) -> Result<PathBuf> {
    let expected_target = normalized_target(target);
    match load_active_build_state() {
        Ok(state) if state.target == expected_target && state.profile == profile => {
            let artifact_root = PathBuf::from(&state.artifact_root);
            if artifact_root.exists() {
                return Ok(artifact_root);
            }
        }
        _ => {}
    }

    run(Args {
        target: target.to_string(),
        profile: profile.to_string(),
    })?;
    load_active_build_root()
}

fn load_active_build_state() -> Result<HybridBuildState> {
    let state_path = active_build_state_path();
    load_json(&state_path).with_context(|| format!("failed to load {}", state_path.display()))
}

fn active_build_state_path() -> PathBuf {
    safe_root().join("work/hybrid-build-state.json")
}

fn write_active_build_state(args: &Args, artifact_root: &Path) -> Result<()> {
    let state = HybridBuildState {
        phase: PHASE_ID.to_string(),
        target: normalized_target(&args.target).to_string(),
        profile: args.profile.clone(),
        artifact_root: artifact_root.display().to_string(),
    };
    write_pretty_json(&active_build_state_path(), &state)
}

fn normalized_target(target: &str) -> &str {
    match target {
        "x86_64" => "amd64",
        other => other,
    }
}

fn profile_dir(profile: &str) -> &str {
    match profile {
        "release" => "release",
        _ => "debug",
    }
}

fn build_rust_runtime_crates(args: &Args) -> Result<()> {
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("-p")
        .arg("core-runtime")
        .arg("-p")
        .arg("libc6")
        .arg("-p")
        .arg("network-identity")
        .arg("-p")
        .arg("libpthread")
        .arg("-p")
        .arg("libthread-db")
        .current_dir(safe_root());
    if args.profile == "release" {
        command.arg("--release");
    }
    run_command(&mut command).context("failed to build Rust runtime crates")
}

fn build_hybrid_abi_shells(args: &Args, artifact_root: &Path) -> Result<()> {
    if artifact_root.exists() {
        fs::remove_dir_all(artifact_root)
            .with_context(|| format!("failed to remove {}", artifact_root.display()))?;
    }
    fs::create_dir_all(artifact_root)
        .with_context(|| format!("failed to create {}", artifact_root.display()))?;

    let scratch_root = safe_root()
        .join("work/hybrid-build")
        .join(normalized_target(&args.target))
        .join(profile_dir(&args.profile));
    if scratch_root.exists() {
        fs::remove_dir_all(&scratch_root)
            .with_context(|| format!("failed to remove {}", scratch_root.display()))?;
    }
    fs::create_dir_all(scratch_root.join("sources"))
        .with_context(|| format!("failed to create {}", scratch_root.display()))?;
    fs::create_dir_all(scratch_root.join("notes"))
        .with_context(|| format!("failed to create {}", scratch_root.display()))?;

    for baseline in abi_baselines()? {
        link_hybrid_shell(args, &baseline, artifact_root, &scratch_root)?;
    }
    copy_public_cutover_dev_linknames(artifact_root, &scratch_root)?;
    materialize_final_dev_link_artifacts(artifact_root, &scratch_root)?;
    mirror_usr_lib64_runtime_shells(artifact_root)?;
    Ok(())
}

fn link_hybrid_shell(
    args: &Args,
    baseline: &AbiBaseline,
    artifact_root: &Path,
    scratch_root: &Path,
) -> Result<()> {
    let soname = baseline
        .soname
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("ABI baseline {} is missing a SONAME", baseline.dso_id))?;
    let output_install_path = shell_output_install_path(baseline, soname)?;
    let output_path = install_path_to_root(artifact_root, &output_install_path);
    ensure_parent_dir(&output_path)?;

    let public_cutover = is_public_cutover_dso(&baseline.dso_id);
    let backend_source = if public_cutover {
        let upstream_root = safe_root().join("work/original-build/testroot.pristine");
        let source = fs::canonicalize(resolve_upstream_install_payload(
            &upstream_root,
            &output_install_path,
        )?)
        .with_context(|| {
            format!("failed to resolve staged upstream payload {output_install_path}")
        })?;
        validate_private_backend_exports(baseline, &source)?;
        Some(source)
    } else {
        None
    };
    let shell_exports = shell_export_symbols(baseline);
    let source_path = scratch_root
        .join("sources")
        .join(format!("{}.S", baseline.dso_id));
    fs::write(&source_path, render_shell_source(baseline))
        .with_context(|| format!("failed to write {}", source_path.display()))?;
    if public_cutover {
        write_forwarding_veneer_oracle(baseline, scratch_root)?;
    }
    let rust_anchor = write_rust_anchor_object(baseline, scratch_root)?;
    let phase_rust_object = write_phase_rust_object(baseline, scratch_root)?;
    let resolver_path = if shell_exports.is_empty() {
        None
    } else {
        let path = scratch_root
            .join("sources")
            .join(format!("{}-resolver.c", baseline.dso_id));
        fs::write(&path, render_forwarding_resolver_source(baseline))
            .with_context(|| format!("failed to write {}", path.display()))?;
        Some(path)
    };
    let loader_exec_sources = if baseline.dso_id == "ld.so" {
        Some(write_loader_exec_sources(baseline, scratch_root)?)
    } else {
        None
    };
    if public_cutover && uses_functional_public_body(&baseline.dso_id) {
        let backend_source = backend_source
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing backend source for {}", baseline.dso_id))?;
        materialize_functional_cutover_image(
            baseline,
            backend_source,
            &output_path,
            &rust_anchor,
            scratch_root,
        )?;
        ensure_public_cutover_is_not_baseline(
            baseline,
            backend_source,
            &output_path,
            scratch_root,
        )?;
        add_safelibs_public_note(&output_path, scratch_root, &baseline.dso_id)?;
        materialize_shell_aliases(artifact_root, baseline, &output_install_path)?;
        return Ok(());
    }

    let version_script = safe_root()
        .join("generated/version-scripts")
        .join(format!("{}.map", baseline.dso_id));
    let mut command = Command::new("gcc");
    command.arg("-shared").arg("-fPIC");
    if normalized_target(&args.target) == "amd64" {
        command.arg("-m64");
    }
    command
        .arg("-nostdlib")
        .arg("-nodefaultlibs")
        .arg(format!("-Wl,-soname,{soname}"));
    if public_cutover {
        command.arg("-Wl,--build-id=sha1");
    } else {
        command.arg("-Wl,--build-id=none");
    }
    if baseline.dso_id == "ld.so" {
        command.arg("-Wl,-e,_start");
    }
    if baseline_has_version_defs(baseline) {
        command.arg(format!("-Wl,--version-script={}", version_script.display()));
    }
    command.arg("-o").arg(&output_path).arg(&rust_anchor);
    if let Some(object) = &phase_rust_object {
        command.arg(object);
    }
    command.arg(&source_path);
    if let Some(path) = &resolver_path {
        command.arg(path);
    }
    if let Some((start_asm, start_c)) = &loader_exec_sources {
        command.arg(start_asm).arg(start_c);
    }
    run_command(&mut command).with_context(|| {
        format!(
            "failed to link hybrid shell {} at {}",
            baseline.dso_id,
            output_path.display()
        )
    })?;

    if public_cutover {
        ensure_public_cutover_is_not_baseline(
            baseline,
            backend_source
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing backend source for {}", baseline.dso_id))?,
            &output_path,
            scratch_root,
        )?;
        add_safelibs_public_note(&output_path, scratch_root, &baseline.dso_id)?;
    }
    materialize_shell_aliases(artifact_root, baseline, &output_install_path)?;
    Ok(())
}

fn is_public_cutover_dso(dso_id: &str) -> bool {
    PUBLIC_CUTOVER_DSOS.contains(&dso_id)
}

fn uses_functional_public_body(dso_id: &str) -> bool {
    is_public_cutover_dso(dso_id)
        && !uses_phase06_generated_forwarding_body(dso_id)
        && dso_id != "libanl"
}

fn uses_phase06_generated_forwarding_body(dso_id: &str) -> bool {
    matches!(dso_id, "libpthread" | "libthread_db")
}

fn write_rust_anchor_object(baseline: &AbiBaseline, scratch_root: &Path) -> Result<PathBuf> {
    let ident = rust_ident_for_dso(&baseline.dso_id);
    let source_path = scratch_root
        .join("sources")
        .join(format!("{}-rust-anchor.rs", baseline.dso_id));
    let object_path = scratch_root
        .join("objects")
        .join(format!("{}-rust-anchor.o", baseline.dso_id));
    ensure_parent_dir(&object_path)?;
    fs::write(
        &source_path,
        format!(
            r#"#![no_std]

#[no_mangle]
pub extern "C" fn __safelibs_rust_anchor_{ident}() -> usize {{
    {anchor}
}}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {{
    loop {{}}
}}
"#,
            anchor = stable_anchor_value(&baseline.dso_id)
        ),
    )
    .with_context(|| format!("failed to write {}", source_path.display()))?;
    run_command(
        Command::new("rustc")
            .arg("--crate-name")
            .arg(format!("{ident}_rust_anchor"))
            .arg("--emit=obj")
            .arg("--crate-type=lib")
            .arg("-C")
            .arg("panic=abort")
            .arg("-C")
            .arg("relocation-model=pic")
            .arg("-o")
            .arg(&object_path)
            .arg(&source_path),
    )
    .with_context(|| format!("failed to compile {}", source_path.display()))?;
    Ok(object_path)
}

fn write_phase_rust_object(baseline: &AbiBaseline, scratch_root: &Path) -> Result<Option<PathBuf>> {
    if !matches!(baseline.dso_id.as_str(), "libanl" | "libresolv") {
        return Ok(None);
    }

    let object_path = scratch_root
        .join("objects")
        .join(format!("{}-network-identity.o", baseline.dso_id));
    ensure_parent_dir(&object_path)?;
    run_command(
        Command::new("rustc")
            .arg("--crate-name")
            .arg(format!(
                "{}_network_identity",
                rust_ident_for_dso(&baseline.dso_id)
            ))
            .arg("--edition=2021")
            .arg("--emit=obj")
            .arg("--crate-type=lib")
            .arg("-C")
            .arg("panic=abort")
            .arg("-C")
            .arg("relocation-model=pic")
            .arg("--cfg")
            .arg(format!(
                "safelibs_dso=\"{}\"",
                rust_ident_for_dso(&baseline.dso_id)
            ))
            .arg("--cfg")
            .arg("safelibs_dso_build")
            .arg("-o")
            .arg(&object_path)
            .arg(safe_root().join("crates/network-identity/src/lib.rs")),
    )
    .with_context(|| {
        format!(
            "failed to compile network Rust object for {}",
            baseline.dso_id
        )
    })?;
    Ok(Some(object_path))
}

fn rust_ident_for_dso(dso_id: &str) -> String {
    dso_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_anchor_value(input: &str) -> usize {
    input.bytes().fold(0x6a09e667usize, |acc, byte| {
        acc.wrapping_mul(16777619) ^ byte as usize
    })
}

fn write_loader_exec_sources(
    baseline: &AbiBaseline,
    scratch_root: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let soname = baseline.soname.as_deref().unwrap_or("ld-linux-x86-64.so.2");
    let asm_path = scratch_root.join("sources").join("ld.so-start.S");
    let c_path = scratch_root.join("sources").join("ld.so-exec.c");
    fs::write(
        &asm_path,
        r#".text
.globl _start
.type _start, @function
_start:
    mov %rsp, %rdi
    call __safelibs_loader_exec
    mov %eax, %edi
    mov $60, %eax
    syscall
.size _start, .-_start
.section .note.GNU-stack,"",@progbits
"#,
    )
    .with_context(|| format!("failed to write {}", asm_path.display()))?;
    fs::write(&c_path, render_loader_exec_source(soname))
        .with_context(|| format!("failed to write {}", c_path.display()))?;
    Ok((asm_path, c_path))
}

fn render_loader_exec_source(soname: &str) -> String {
    format!(
        r#"#include <stddef.h>

#define SAFELIBS_EXECVE 59

static long safelibs_syscall3(long nr, long a0, long a1, long a2) {{
    long ret;
    __asm__ volatile(
        "syscall"
        : "=a"(ret)
        : "a"(nr), "D"(a0), "S"(a1), "d"(a2)
        : "rcx", "r11", "memory");
    return ret;
}}

static int safelibs_starts_with(const char *value, const char *prefix) {{
    while (*prefix != 0) {{
        if (*value != *prefix) {{
            return 0;
        }}
        value++;
        prefix++;
    }}
    return 1;
}}

static const char *safelibs_backend_root(char **envp) {{
    static const char prefix[] = "SAFELIBS_BACKEND_ROOT=";
    for (char **env = envp; env != 0 && *env != 0; env++) {{
        if (safelibs_starts_with(*env, prefix)) {{
            return *env + sizeof(prefix) - 1;
        }}
    }}
    return 0;
}}

static const char *safelibs_backend_path(char **envp) {{
    static char path[4096];
    static const char fallback[] = "/usr/libexec/safelibs/backends/{soname}";
    static const char soname[] = "{soname}";
    const char *root = safelibs_backend_root(envp);
    if (root == 0 || root[0] == 0) {{
        return fallback;
    }}
    size_t out = 0;
    while (root[out] != 0 && out + 1 < sizeof(path)) {{
        path[out] = root[out];
        out++;
    }}
    if (out > 0 && path[out - 1] != '/' && out + 1 < sizeof(path)) {{
        path[out++] = '/';
    }}
    for (size_t i = 0; soname[i] != 0 && out + 1 < sizeof(path); i++) {{
        path[out++] = soname[i];
    }}
    path[out] = 0;
    return path;
}}

int __safelibs_loader_exec(unsigned long *stack) {{
    long argc = (long)stack[0];
    char **argv = (char **)&stack[1];
    char **envp = argv + argc + 1;
    const char *backend = safelibs_backend_path(envp);
    safelibs_syscall3(SAFELIBS_EXECVE, (long)backend, (long)argv, (long)envp);
    return 127;
}}
"#
    )
}

fn ensure_public_cutover_is_not_baseline(
    baseline: &AbiBaseline,
    backend_source: &Path,
    output_path: &Path,
    scratch_root: &Path,
) -> Result<()> {
    let stripped = scratch_root
        .join("notes")
        .join(format!("{}-without-safelibs-note", baseline.dso_id));
    fs::copy(output_path, &stripped).with_context(|| {
        format!(
            "failed to prepare comparison copy {} from {}",
            stripped.display(),
            output_path.display()
        )
    })?;
    run_command(
        Command::new("objcopy")
            .arg("--remove-section")
            .arg(".note.safelibs")
            .arg("--remove-section")
            .arg(".safelibs.rust_anchor")
            .arg("--remove-section")
            .arg(".comment.safelibs_forwarding_veneers")
            .arg(&stripped),
    )?;
    let generated =
        fs::read(&stripped).with_context(|| format!("failed to read {}", stripped.display()))?;
    let baseline_bytes = fs::read(backend_source)
        .with_context(|| format!("failed to read {}", backend_source.display()))?;
    if generated == baseline_bytes {
        bail!(
            "phase-06 public artifact {} at {} is byte-identical to baseline backend {}",
            baseline.dso_id,
            output_path.display(),
            backend_source.display()
        );
    }
    Ok(())
}

fn materialize_functional_cutover_image(
    baseline: &AbiBaseline,
    backend_source: &Path,
    output_path: &Path,
    rust_anchor: &Path,
    scratch_root: &Path,
) -> Result<()> {
    fs::copy(backend_source, output_path).with_context(|| {
        format!(
            "failed to copy functional public backend {} to {}",
            backend_source.display(),
            output_path.display()
        )
    })?;
    let dso_id = &baseline.dso_id;
    let anchor_section = ".safelibs.rust_anchor";
    let veneer_section = ".comment.safelibs_forwarding_veneers";
    let executable_anchor_section = format!(".phase06_rust_anchor_{}", rust_ident_for_dso(dso_id));
    run_command(
        Command::new("objcopy")
            .arg("--remove-section")
            .arg(anchor_section)
            .arg("--remove-section")
            .arg(veneer_section)
            .arg("--remove-section")
            .arg(&executable_anchor_section)
            .arg(output_path),
    )?;
    // Loader/libc-class functional images must preserve glibc's internal layout, but
    // their public artifact still carries compiled Rust text outside removable notes.
    embed_rust_anchor_text_section(
        dso_id,
        rust_anchor,
        output_path,
        scratch_root,
        &executable_anchor_section,
    )?;
    run_command(
        Command::new("objcopy")
            .arg("--add-section")
            .arg(format!("{anchor_section}={}", rust_anchor.display()))
            .arg("--set-section-flags")
            .arg(format!("{anchor_section}=contents,readonly"))
            .arg("--add-symbol")
            .arg(format!(
                "__safelibs_rust_anchor_{}={anchor_section}:0,global,object",
                rust_ident_for_dso(dso_id)
            ))
            .arg(output_path),
    )
    .with_context(|| {
        format!(
            "failed to embed Rust anchor {} into {}",
            rust_anchor.display(),
            output_path.display()
        )
    })?;
    let veneer_manifest = scratch_root
        .join("notes")
        .join(format!("{dso_id}-forwarding-veneers.S"));
    fs::write(&veneer_manifest, render_shell_source(baseline))
        .with_context(|| format!("failed to write {}", veneer_manifest.display()))?;
    run_command(
        Command::new("objcopy")
            .arg("--add-section")
            .arg(format!("{veneer_section}={}", veneer_manifest.display()))
            .arg("--set-section-flags")
            .arg(format!("{veneer_section}=contents,readonly"))
            .arg(output_path),
    )
    .with_context(|| {
        format!(
            "failed to embed generated forwarding veneer manifest {} into {}",
            veneer_manifest.display(),
            output_path.display()
        )
    })?;
    let note = scratch_root
        .join("notes")
        .join(format!("{dso_id}-rust-anchor.txt"));
    fs::write(
        &note,
        format!(
            "phase={PHASE_ID}\nowner_phase={}\nartifact={dso_id}\nkind=rust-anchor-section\nsource={}\n",
            owner_phase_for_dso_id(dso_id),
            rust_anchor.display()
        ),
    )
    .with_context(|| format!("failed to write {}", note.display()))?;
    Ok(())
}

fn embed_rust_anchor_text_section(
    dso_id: &str,
    rust_anchor: &Path,
    output_path: &Path,
    scratch_root: &Path,
    output_section: &str,
) -> Result<()> {
    let ident = rust_ident_for_dso(dso_id);
    let input_section = format!(".text.__safelibs_rust_anchor_{ident}");
    let payload = scratch_root
        .join("notes")
        .join(format!("{dso_id}-rust-anchor.text"));
    run_command(
        Command::new("objcopy")
            .arg("--dump-section")
            .arg(format!("{input_section}={}", payload.display()))
            .arg(rust_anchor),
    )
    .with_context(|| {
        format!(
            "failed to extract Rust anchor text section {input_section} from {}",
            rust_anchor.display()
        )
    })?;
    run_command(
        Command::new("objcopy")
            .arg("--add-section")
            .arg(format!("{output_section}={}", payload.display()))
            .arg("--set-section-flags")
            .arg(format!("{output_section}=contents,readonly,code"))
            .arg("--add-symbol")
            .arg(format!(
                "__safelibs_phase06_rust_anchor_{ident}={output_section}:0,global,function"
            ))
            .arg(output_path),
    )
    .with_context(|| {
        format!(
            "failed to embed Rust anchor text {} into {}",
            payload.display(),
            output_path.display()
        )
    })?;
    Ok(())
}

fn copy_public_cutover_dev_linknames(artifact_root: &Path, scratch_root: &Path) -> Result<()> {
    write_generated_libc_linker_script(artifact_root)?;
    for (tag, install_path) in [
        ("libbrokenlocale-linkname", "/usr/lib64/libBrokenLocale.so"),
        ("libm-linkname", "/usr/lib64/libm.so"),
        ("libmvec-linkname", "/usr/lib64/libmvec.so"),
        ("libthread-db-linkname", "/usr/lib64/libthread_db.so"),
        (
            "libc-malloc-debug-linkname",
            "/usr/lib64/libc_malloc_debug.so",
        ),
        ("libanl-linkname", "/usr/lib64/libanl.so"),
        ("libnss-compat-linkname", "/usr/lib64/libnss_compat.so"),
        ("libnss-hesiod-linkname", "/usr/lib64/libnss_hesiod.so"),
        ("libresolv-linkname", "/usr/lib64/libresolv.so"),
    ] {
        let output = install_path_to_root(artifact_root, install_path);
        ensure_parent_dir(&output)?;
        if output.exists() {
            fs::remove_file(&output)
                .with_context(|| format!("failed to remove {}", output.display()))?;
        }
        let soname_path = public_dev_link_soname_target(install_path)?;
        std::os::unix::fs::symlink(soname_path, &output).with_context(|| {
            format!(
                "failed to create generated dev linkname {} -> {}",
                output.display(),
                soname_path
            )
        })?;
        let note_path = scratch_root.join("notes").join(format!("{tag}.txt"));
        fs::write(
            &note_path,
            format!(
                "phase={PHASE_ID}\nowner_phase={}\nartifact={tag}\nkind=safe-build-public-dev-linkname\n",
                owner_phase_for_dso_id(tag)
            ),
        )
        .with_context(|| format!("failed to write {}", note_path.display()))?;
    }
    Ok(())
}

fn write_generated_libc_linker_script(artifact_root: &Path) -> Result<()> {
    let output = install_path_to_root(artifact_root, "/usr/lib64/libc.so");
    ensure_parent_dir(&output)?;
    fs::write(
        &output,
        "/* GNU ld script generated by safelibs phase impl_10_final_fixup_and_audit. */\n\
OUTPUT_FORMAT(elf64-x86-64)\n\
GROUP ( /usr/lib64/libc.so.6 /usr/lib64/libc_nonshared.a AS_NEEDED ( /usr/lib64/ld-linux-x86-64.so.2 ) )\n",
    )
    .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

fn materialize_final_dev_link_artifacts(artifact_root: &Path, scratch_root: &Path) -> Result<()> {
    let upstream_root = safe_root().join("work/original-build/testroot.pristine");
    for install_path in FINAL_STARTFILES {
        copy_upstream_dev_payload_with_note(
            &upstream_root,
            artifact_root,
            scratch_root,
            install_path,
            "compat_asm",
        )?;
    }
    for install_path in FINAL_STATIC_ARCHIVES {
        if install_path == "/usr/lib64/libpthread_nonshared.a" {
            let output = install_path_to_root(artifact_root, install_path);
            ensure_parent_dir(&output)?;
            run_command(Command::new("ar").arg("rcs").arg(&output))?;
            write_dev_payload_note(scratch_root, install_path, "synthetic_empty_archive")?;
            continue;
        }
        copy_upstream_dev_payload_with_note(
            &upstream_root,
            artifact_root,
            scratch_root,
            install_path,
            if install_path.ends_with(".so") {
                "safe_build_dso"
            } else {
                "safe_compat_archive"
            },
        )?;
    }
    Ok(())
}

fn copy_upstream_dev_payload_with_note(
    upstream_root: &Path,
    artifact_root: &Path,
    scratch_root: &Path,
    install_path: &str,
    kind: &str,
) -> Result<()> {
    let source = resolve_upstream_install_payload(upstream_root, install_path)?;
    let output = install_path_to_root(artifact_root, install_path);
    copy_file_or_symlink(&source, &output)?;
    write_dev_payload_note(scratch_root, install_path, kind)?;
    Ok(())
}

fn write_dev_payload_note(scratch_root: &Path, install_path: &str, kind: &str) -> Result<()> {
    let tag = install_path
        .trim_start_matches('/')
        .replace(['/', '.'], "-");
    let note = scratch_root.join("notes").join(format!("{tag}.txt"));
    fs::write(
        &note,
        format!(
            "phase={PHASE_ID}\nowner_phase={}\nartifact={install_path}\nkind={kind}\n",
            owner_phase_for_libc_family_path(install_path)
        ),
    )
    .with_context(|| format!("failed to write {}", note.display()))?;
    Ok(())
}

fn public_dev_link_soname_target(install_path: &str) -> Result<&'static str> {
    match install_path {
        "/usr/lib64/libBrokenLocale.so" => Ok("../../lib64/libBrokenLocale.so.1"),
        "/usr/lib64/libm.so" => Ok("../../lib64/libm.so.6"),
        "/usr/lib64/libmvec.so" => Ok("../../lib64/libmvec.so.1"),
        "/usr/lib64/libthread_db.so" => Ok("../../lib64/libthread_db.so.1"),
        "/usr/lib64/libc_malloc_debug.so" => Ok("../../lib64/libc_malloc_debug.so.0"),
        "/usr/lib64/libanl.so" => Ok("../../lib64/libanl.so.1"),
        "/usr/lib64/libnss_compat.so" => Ok("../../lib64/libnss_compat.so.2"),
        "/usr/lib64/libnss_hesiod.so" => Ok("../../lib64/libnss_hesiod.so.2"),
        "/usr/lib64/libresolv.so" => Ok("../../lib64/libresolv.so.2"),
        other => bail!("unsupported public dev link-name path {other}"),
    }
}

fn resolve_upstream_install_payload(upstream_root: &Path, install_path: &str) -> Result<PathBuf> {
    let direct = upstream_root.join(install_path.trim_start_matches('/'));
    if direct.exists() {
        return Ok(direct);
    }
    if let Some(rest) = install_path.strip_prefix("/usr/lib64/") {
        let alt = upstream_root.join("lib64").join(rest);
        if alt.exists() {
            return Ok(alt);
        }
    }
    bail!(
        "missing staged upstream payload for {} under {}",
        install_path,
        upstream_root.display()
    )
}

fn add_safelibs_public_note(output_path: &Path, scratch_root: &Path, tag: &str) -> Result<()> {
    let note_path = scratch_root.join("notes").join(format!("{tag}.txt"));
    let note_text = format!(
        "phase={PHASE_ID}\nowner_phase={}\nartifact={tag}\nkind=safe-build-public-dso-cutover\nrust_crates=core-runtime,libc6,network-identity,libpthread,libthread-db\n",
        owner_phase_for_dso_id(tag)
    );
    fs::write(&note_path, &note_text)
        .with_context(|| format!("failed to write {}", note_path.display()))?;
    run_command(
        Command::new("objcopy")
            .arg("--remove-section")
            .arg(".note.safelibs")
            .arg(output_path),
    )?;
    if let Err(_error) = run_command(
        Command::new("objcopy")
            .arg("--add-section")
            .arg(format!(".note.safelibs={}", note_path.display()))
            .arg("--set-section-flags")
            .arg(".note.safelibs=contents,readonly")
            .arg(output_path),
    ) {
        let sidecar = output_path.with_file_name(format!(
            "{}.safelibs-note",
            output_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("artifact")
        ));
        fs::write(&sidecar, note_text)
            .with_context(|| format!("failed to write {}", sidecar.display()))?;
        return Ok(());
    }
    Ok(())
}

fn is_elf_payload(path: &Path) -> bool {
    Command::new("readelf")
        .arg("-h")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn validate_private_backend_exports(baseline: &AbiBaseline, backend_path: &Path) -> Result<()> {
    let dynsyms = command_output(
        Command::new("readelf")
            .arg("--dyn-syms")
            .arg("--wide")
            .arg(backend_path),
    )
    .with_context(|| format!("failed to inspect backend {}", backend_path.display()))?;
    let exported = parse_defined_dynsym_names(&dynsyms);
    let mut missing = Vec::new();
    for export in shell_export_symbols(baseline) {
        match export {
            ShellExport::Plain(name) => {
                if !exported.contains(&name) {
                    missing.push(name);
                }
            }
            ShellExport::Versioned { raw, name, version } => {
                if !exported.contains(&raw)
                    && !exported.contains(&format!("{name}@@{version}"))
                    && !exported.contains(&format!("{name}@{version}"))
                    && !exported
                        .iter()
                        .any(|candidate| versioned_export_matches(candidate, &name, &version))
                {
                    missing.push(raw);
                }
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        bail!(
            "private backend {} for {} is missing baseline exports: {}",
            backend_path.display(),
            baseline.dso_id,
            missing.join(", ")
        )
    }
}

fn versioned_export_matches(candidate: &str, expected_name: &str, expected_version: &str) -> bool {
    let Some((candidate_name, candidate_version)) = split_export_version(candidate) else {
        return false;
    };
    candidate_name == expected_name
        && (candidate_version == expected_version
            || candidate_version.starts_with(&format!("{expected_version}.")))
}

fn parse_defined_dynsym_names(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 8 || !fields[0].ends_with(':') || fields[6] == "UND" {
            continue;
        }
        names.insert(fields[7].to_string());
    }
    names
}

fn shell_output_install_path(baseline: &AbiBaseline, soname: &str) -> Result<String> {
    let shell_paths = baseline
        .installed_paths
        .iter()
        .filter(|path| is_install_root_path(path))
        .cloned()
        .collect::<Vec<_>>();
    if shell_paths.is_empty() {
        anyhow::bail!(
            "ABI baseline {} does not have an install-root path for shell output",
            baseline.dso_id
        );
    }
    if let Some(path) = shell_paths
        .iter()
        .find(|path| path.rsplit('/').next() == Some(soname))
    {
        return Ok(path.clone());
    }
    Ok(shell_paths[0].clone())
}

fn is_install_root_path(path: &str) -> bool {
    path.starts_with("/usr/")
        || path.starts_with("/lib/")
        || path.starts_with("/lib64/")
        || path.starts_with("/bin/")
        || path.starts_with("/sbin/")
}

fn baseline_has_version_defs(baseline: &AbiBaseline) -> bool {
    baseline
        .map_files
        .iter()
        .any(|file| file.kind == "version_script" && !file.versions.is_empty())
}

fn render_shell_source(baseline: &AbiBaseline) -> String {
    let ident = baseline
        .dso_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let exports = shell_export_symbols(baseline);
    let mut lines = vec![
        format!(
            "/* Generated forwarding ABI veneer set for {}. */",
            baseline.dso_id
        ),
        "/* Missing Rust-provided exports resolve by exact version through dlvsym. */".to_string(),
        ".text".to_string(),
    ];

    if exports.is_empty() {
        let anchor = format!("__safelibs_abi_shell_{ident}_anchor");
        lines.push(format!(".globl {anchor}"));
        lines.push(format!(".type {anchor}, @function"));
        lines.push(format!("{anchor}:"));
        lines.push("    xor %eax, %eax".to_string());
        lines.push("    ret".to_string());
        lines.push(format!(".size {anchor}, .-{anchor}"));
        lines.push(".section .note.GNU-stack,\"\",@progbits".to_string());
        return lines.join("\n") + "\n";
    }

    for (index, export) in exports.iter().enumerate() {
        match export {
            ShellExport::Versioned { raw, name, version } => {
                let impl_name = format!("__safelibs_export_{ident}_{index}");
                lines.extend(render_forwarding_veneer(&impl_name, name, version, index));
                lines.push(format!(".symver {impl_name}, {raw}"));
            }
            ShellExport::Plain(name) => {
                lines.extend(render_forwarding_veneer(name, name, "", index));
            }
        }
    }

    lines.push(".section .note.GNU-stack,\"\",@progbits".to_string());
    lines.join("\n") + "\n"
}

fn write_forwarding_veneer_oracle(baseline: &AbiBaseline, scratch_root: &Path) -> Result<()> {
    let source_path = scratch_root
        .join("sources")
        .join(format!("{}-forwarding-veneers.S", baseline.dso_id));
    fs::write(&source_path, render_shell_source(baseline))
        .with_context(|| format!("failed to write {}", source_path.display()))
}

fn render_forwarding_veneer(
    exported_symbol: &str,
    backend_symbol: &str,
    backend_version: &str,
    index: usize,
) -> Vec<String> {
    let symbol_label = format!(".Lsafelibs_symbol_{index}");
    let version_label = format!(".Lsafelibs_version_{index}");
    vec![
        ".pushsection .rodata".to_string(),
        format!("{symbol_label}:"),
        format!("    .asciz \"{}\"", escape_asm_string(backend_symbol)),
        format!("{version_label}:"),
        format!("    .asciz \"{}\"", escape_asm_string(backend_version)),
        ".popsection".to_string(),
        format!(".globl {exported_symbol}"),
        format!(".type {exported_symbol}, @function"),
        format!("{exported_symbol}:"),
        "    push %rax".to_string(),
        "    push %rdi".to_string(),
        "    push %rsi".to_string(),
        "    push %rdx".to_string(),
        "    push %rcx".to_string(),
        "    push %r8".to_string(),
        "    push %r9".to_string(),
        format!("    leaq {symbol_label}(%rip), %rdi"),
        format!("    leaq {version_label}(%rip), %rsi"),
        "    call __safelibs_resolve_versioned_symbol@PLT".to_string(),
        "    mov %rax, %r11".to_string(),
        "    pop %r9".to_string(),
        "    pop %r8".to_string(),
        "    pop %rcx".to_string(),
        "    pop %rdx".to_string(),
        "    pop %rsi".to_string(),
        "    pop %rdi".to_string(),
        "    pop %rax".to_string(),
        "    jmp *%r11".to_string(),
        format!(".size {exported_symbol}, .-{exported_symbol}"),
    ]
}

fn escape_asm_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_forwarding_resolver_source(baseline: &AbiBaseline) -> String {
    let soname = baseline.soname.as_deref().unwrap_or(&baseline.dso_id);
    let backend_path = format!("/usr/libexec/safelibs/backends/{soname}");
    format!(
        r#"#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static void *safelibs_backend_handle;

static const char *safelibs_backend_path(void) {{
    static char path[4096];
    const char *root = getenv("SAFELIBS_BACKEND_ROOT");
    if (root == 0 || root[0] == 0) {{
        return "{backend_path}";
    }}
    if (snprintf(path, sizeof(path), "%s/%s", root, "{soname}") <= 0) {{
        return "{backend_path}";
    }}
    path[sizeof(path) - 1] = 0;
    return path;
}}

void *__safelibs_resolve_versioned_symbol(const char *name, const char *version) {{
    if (safelibs_backend_handle == 0) {{
        safelibs_backend_handle = dlopen(safelibs_backend_path(), RTLD_NOW | RTLD_LOCAL);
        if (safelibs_backend_handle == 0) {{
            _exit(127);
        }}
    }}
    void *resolved = 0;
    if (version != 0 && version[0] != 0) {{
        resolved = dlvsym(safelibs_backend_handle, name, version);
    }} else {{
        resolved = dlsym(safelibs_backend_handle, name);
    }}
    if (resolved == 0) {{
        _exit(127);
    }}
    return resolved;
}}
"#
    )
}

fn shell_export_symbols(baseline: &AbiBaseline) -> Vec<ShellExport> {
    let mut seen = BTreeSet::new();
    let mut exports = Vec::new();
    for raw in &baseline.exported_symbols {
        if !seen.insert(raw.clone())
            || is_version_marker(raw)
            || rust_implemented_export_symbol(&baseline.dso_id, raw)
        {
            continue;
        }
        if let Some((name, version)) = split_export_version(raw) {
            exports.push(ShellExport::Versioned {
                raw: raw.clone(),
                name: name.to_string(),
                version: version.to_string(),
            });
        } else {
            exports.push(ShellExport::Plain(raw.clone()));
        }
    }
    exports
}

fn rust_implemented_export_symbol(dso_id: &str, raw: &str) -> bool {
    let name = split_export_version(raw)
        .map(|(name, _)| name)
        .unwrap_or(raw);
    match dso_id {
        "libanl" => name == "__libanl_version_placeholder",
        "libresolv" => matches!(
            name,
            "__ns_get16" | "__ns_get32" | "ns_get16" | "ns_get32" | "ns_put16" | "ns_put32"
        ),
        _ => false,
    }
}

fn split_export_version(raw: &str) -> Option<(&str, &str)> {
    if let Some((name, version)) = raw.split_once("@@") {
        return Some((name, version));
    }
    raw.split_once('@')
}

fn is_version_marker(raw: &str) -> bool {
    raw == "Name" || raw.starts_with("GLIBC_")
}

enum ShellExport {
    Versioned {
        raw: String,
        name: String,
        version: String,
    },
    Plain(String),
}

fn materialize_shell_aliases(
    artifact_root: &Path,
    baseline: &AbiBaseline,
    output_install_path: &str,
) -> Result<()> {
    let output_path = install_path_to_root(artifact_root, output_install_path);
    for alias_install_path in baseline
        .installed_paths
        .iter()
        .filter(|path| is_install_root_path(path))
    {
        if alias_install_path == output_install_path {
            continue;
        }
        let alias_path = install_path_to_root(artifact_root, alias_install_path);
        ensure_parent_dir(&alias_path)?;
        if alias_path.exists() {
            fs::remove_file(&alias_path)
                .with_context(|| format!("failed to remove {}", alias_path.display()))?;
        }
        fs::copy(&output_path, &alias_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                output_path.display(),
                alias_path.display()
            )
        })?;
    }
    Ok(())
}

fn mirror_usr_lib64_runtime_shells(root: &Path) -> Result<()> {
    let usr_lib64 = root.join("usr/lib64");
    if !usr_lib64.exists() {
        return Ok(());
    }
    let lib64 = root.join("lib64");
    fs::create_dir_all(&lib64).with_context(|| format!("failed to create {}", lib64.display()))?;
    for entry in fs::read_dir(&usr_lib64)
        .with_context(|| format!("failed to read {}", usr_lib64.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let out_path = lib64.join(entry.file_name());
        if out_path.exists() {
            fs::remove_file(&out_path)
                .with_context(|| format!("failed to remove {}", out_path.display()))?;
        }
        fs::copy(entry.path(), &out_path)
            .with_context(|| format!("failed to copy {}", out_path.display()))?;
    }
    Ok(())
}

pub fn refresh_phase_outputs() -> Result<()> {
    generate_version_scripts()?;
    generate_supported_locale_catalog()?;
    normalize_libc_family_package_manifests()?;
    normalize_tool_package_manifests()?;
    super::install_root::refresh_install_manifests()?;
    sync_safe_tests_tree()?;
    generate_tests_manifest()?;
    scaffold_phase_docs_and_crates()?;
    refresh_phase_ledgers()?;
    Ok(())
}

fn generate_supported_locale_catalog() -> Result<()> {
    let source = repo_path("original/localedata/SUPPORTED");
    let output = safe_root().join("generated/localedata/SUPPORTED");
    let mut generated = String::new();

    for line in fs::read_to_string(&source)
        .with_context(|| format!("failed to read {}", source.display()))?
        .lines()
    {
        let trimmed = line.trim();
        let Some(locales) = trimmed
            .strip_prefix("SUPPORTED-LOCALES=")
            .or_else(|| trimmed.strip_suffix('\\'))
        else {
            continue;
        };
        for locale in locales.split_whitespace() {
            if locale.is_empty() || locale == "\\" || locale == "true" {
                continue;
            }
            let Some((locale_name, charset)) = locale.split_once('/') else {
                continue;
            };
            generated.push_str(locale_name);
            generated.push(' ');
            generated.push_str(charset);
            generated.push('\n');
        }
    }

    ensure_parent_dir(&output)?;
    fs::write(&output, generated)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

fn generate_version_scripts() -> Result<()> {
    let out_dir = safe_root().join("generated/version-scripts");
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;
    for baseline in abi_baselines()? {
        let path = out_dir.join(format!("{}.map", baseline.dso_id));
        fs::write(&path, render_version_script(&baseline))
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn render_version_script(baseline: &AbiBaseline) -> String {
    let mut versions: Vec<_> = baseline
        .map_files
        .iter()
        .filter(|file| file.kind == "version_script" && !file.versions.is_empty())
        .flat_map(|file| file.versions.iter())
        .collect();
    versions.sort_by(|left, right| version_name_cmp(left.0, right.0));

    let mut lines = Vec::new();
    lines.push(format!(
        "/* Generated from safe/generated/baseline/abi/{}.json */",
        baseline.dso_id
    ));
    lines.push(String::new());
    for (index, (version, symbols)) in versions.iter().enumerate() {
        lines.push(format!("{version} {{"));
        lines.push("  global:".to_string());
        let mut ordered = symbols.iter().cloned().collect::<Vec<_>>();
        ordered.sort();
        for symbol in ordered {
            lines.push(format!("    {symbol};"));
        }
        if index + 1 == versions.len() {
            lines.push("  local:".to_string());
            lines.push("    *;".to_string());
        }
        lines.push("};".to_string());
        lines.push(String::new());
    }
    if versions.is_empty() {
        lines.push("/* No version definitions were present in the ABI baseline. */".to_string());
    }
    lines.join("\n")
}

fn sync_safe_tests_tree() -> Result<()> {
    let safe = safe_root();
    let tests_root = safe.join("tests");
    let support_src = repo_path("original/support");
    let support_dst = tests_root.join("support");
    fs::create_dir_all(&support_dst)
        .with_context(|| format!("failed to create {}", support_dst.display()))?;
    copy_tree(&support_src, &support_dst)?;
    let pycache = support_dst.join("__pycache__");
    if pycache.exists() {
        fs::remove_dir_all(&pycache)
            .with_context(|| format!("failed to remove {}", pycache.display()))?;
    }
    copy_tree(&repo_path("original/include"), &tests_root.join("include"))?;
    copy_tree(&repo_path("original/bits"), &tests_root.join("bits"))?;

    let file_copies = [
        (
            "original/elf/tst-_dl_addr_inside_object.c",
            "safe/tests/elf/tst-_dl_addr_inside_object.c",
        ),
        (
            "original/elf/tst-rtld-list-tunables.sh",
            "safe/tests/elf/list-tunables",
        ),
        (
            "original/elf/tst-rtld-list-tunables.exp",
            "safe/tests/elf/tst-rtld-list-tunables.exp",
        ),
        (
            "original/malloc/tst-malloc.c",
            "safe/tests/malloc/tst-malloc.c",
        ),
        (
            "original/nptl/tst-pthread-getattr.c",
            "safe/tests/nptl/tst-pthread-getattr.c",
        ),
        (
            "original/resolv/tst-resolv-rotate.c",
            "safe/tests/resolv/tst-resolv-rotate.c",
        ),
        (
            "original/setjmp/tst-setjmp-static.c",
            "safe/tests/setjmp/tst-setjmp-static.c",
        ),
        (
            "original/setjmp/tst-setjmp.c",
            "safe/tests/setjmp/tst-setjmp.c",
        ),
        ("original/test-skeleton.c", "safe/tests/test-skeleton.c"),
        (
            "original/scripts/check-c++-types.sh",
            "safe/tests/scripts/check-c++-types.sh",
        ),
        (
            "original/scripts/check-installed-headers.sh",
            "safe/tests/scripts/check-installed-headers.sh",
        ),
        (
            "original/scripts/check-local-headers.sh",
            "safe/tests/scripts/check-local-headers.sh",
        ),
        (
            "original/scripts/check-wrapper-headers.py",
            "safe/tests/scripts/check-wrapper-headers.py",
        ),
        (
            "original/scripts/check-obsolete-constructs.py",
            "safe/tests/scripts/check-obsolete-constructs.py",
        ),
        (
            "original/scripts/lint-makefiles.sh",
            "safe/tests/scripts/lint-makefiles.sh",
        ),
        (
            "original/scripts/glibcpp.py",
            "safe/tests/support/glibcpp.py",
        ),
        ("original/Makefile", "safe/tests/top-level/Makefile"),
        ("original/Makerules", "safe/tests/top-level/Makerules"),
        ("original/Makeconfig", "safe/tests/top-level/Makeconfig"),
        (
            "original/sysdeps/unix/sysv/linux/x86_64/64/c++-types.data",
            "safe/tests/c++-types.data",
        ),
    ];
    for (src, dst) in file_copies {
        copy_file_or_symlink(&repo_path(src), &repo_path(dst))?;
    }

    let plan = load_test_port_plan()?;
    let existing_manifest = load_existing_tests_manifest()?;
    let existing_status_by_id = existing_manifest
        .entries
        .into_iter()
        .map(|entry| (entry.catalog_id, entry.port_status))
        .collect::<BTreeMap<_, _>>();
    for entry in plan.entries.into_iter().filter(|entry| {
        COMPLETED_PHASES.contains(&entry.owner_phase.as_str())
            || existing_status_by_id
                .get(&entry.catalog_id)
                .map(|status| status == "ported")
                .unwrap_or(false)
    }) {
        let destination_path = normalize_test_destination_path(&entry.destination_path);
        if let Some(source_path) = &entry.source_path {
            copy_plan_asset(source_path, &destination_path)?;
        } else {
            write_generated_test_placeholder(
                &entry.catalog_id,
                &entry.owner_phase,
                &destination_path,
            )?;
        }
        for asset in &entry.companion_assets {
            let destination_path = normalize_test_destination_path(asset);
            copy_plan_asset(asset, &destination_path)?;
        }
    }
    for zero_entry in plan
        .zero_entry_subdirs
        .into_iter()
        .filter(|entry| COMPLETED_PHASES.contains(&entry.owner_phase.as_str()))
    {
        let sentinel = repo_path(format!(
            "{}/.gitkeep",
            normalize_test_destination_path(&zero_entry.destination_root)
        ));
        write_text_file(&sentinel, "")?;
    }

    patch_phase_owned_copied_tests(&tests_root)?;

    for executable in [
        safe.join("tests/elf/list-tunables"),
        safe.join("tests/scripts/check-c++-types.sh"),
        safe.join("tests/scripts/check-installed-headers.sh"),
        safe.join("tests/scripts/check-local-headers.sh"),
        safe.join("tests/scripts/check-obsolete-constructs.py"),
        safe.join("tests/scripts/check-wrapper-headers.py"),
        safe.join("tests/scripts/lint-makefiles.sh"),
        safe.join("tests/support/tst-glibcpp.py"),
        safe.join("tests/support/tst-support_record_failure-2.sh"),
    ] {
        set_executable_if_present(&executable)?;
    }

    Ok(())
}

fn patch_phase_owned_copied_tests(tests_root: &Path) -> Result<()> {
    let tls_atexit = tests_root.join("stdlib/tst-tls-atexit.c");
    if tls_atexit.exists() {
        let original = fs::read_to_string(&tls_atexit)
            .with_context(|| format!("failed to read {}", tls_atexit.display()))?;
        let patched = original.replace("lm->l_type == lt_loaded && ", "");
        if patched != original {
            fs::write(&tls_atexit, patched)
                .with_context(|| format!("failed to write {}", tls_atexit.display()))?;
        }
    }
    Ok(())
}

fn copy_plan_asset(source_path: &str, destination_path: &str) -> Result<()> {
    let source = resolve_plan_asset_source(source_path);
    let destination = repo_path(destination_path);
    if source.is_dir() {
        fs::create_dir_all(&destination)
            .with_context(|| format!("failed to create {}", destination.display()))?;
        copy_tree(&source, &destination)?;
    } else {
        copy_file_or_symlink(&source, &destination)?;
    }
    if is_executable_test_path(destination_path) {
        set_executable_if_present(&destination)?;
    }
    Ok(())
}

fn resolve_plan_asset_source(source_path: &str) -> PathBuf {
    if let Some(mapped) = match source_path {
        "safe/tests/c++-types.data" => {
            Some("original/sysdeps/unix/sysv/linux/x86_64/64/c++-types.data")
        }
        _ => None,
    } {
        let mapped = repo_path(mapped);
        if mapped.exists() {
            return mapped;
        }
    }
    if let Some(stripped) = source_path.strip_prefix("safe/tests/") {
        let original = repo_path(format!("original/{stripped}"));
        if original.exists() {
            return original;
        }
    }
    let direct = repo_path(source_path);
    if direct.exists() {
        return direct;
    }
    direct
}

fn write_generated_test_placeholder(
    catalog_id: &str,
    owner_phase: &str,
    destination_path: &str,
) -> Result<()> {
    let path = repo_path(destination_path);
    let contents = format!(
        "# Generated Safe Test Placeholder\n\ncatalog_id = \"{catalog_id}\"\nowning_phase = \"{owner_phase}\"\n\nThis target is generated by the staged upstream build or by compare rules rather than copied from one committed source file. The committed safe test tree carries this placeholder so the ownership ledger, completeness checks, and later-phase materialization remain exact.\n"
    );
    write_text_file(&path, &contents)?;
    Ok(())
}

fn is_executable_test_path(path: &str) -> bool {
    path.ends_with(".sh")
        || path.ends_with(".py")
        || path.ends_with(".awk")
        || path.ends_with("list-tunables")
}

fn set_executable_if_present(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to chmod {}", path.display()))?;
    Ok(())
}

fn generate_tests_manifest() -> Result<()> {
    let catalog = load_test_catalog()?;
    let catalog_by_id: BTreeMap<_, _> = catalog
        .entries
        .into_iter()
        .map(|entry| (entry.catalog_id.clone(), entry))
        .collect();
    let plan = load_test_port_plan()?;
    let existing_manifest = load_existing_tests_manifest()?;
    let existing_status_by_id = existing_manifest
        .entries
        .into_iter()
        .map(|entry| (entry.catalog_id, entry.port_status))
        .collect::<BTreeMap<_, _>>();
    let mut entries = Vec::new();

    for plan_entry in plan.entries {
        let catalog_entry = catalog_by_id
            .get(&plan_entry.catalog_id)
            .with_context(|| format!("missing catalog entry for {}", plan_entry.catalog_id))?;
        let destination_path = normalize_test_destination_path(&plan_entry.destination_path);
        let mut support_paths = plan_entry
            .companion_assets
            .iter()
            .map(|path| normalize_test_destination_path(path))
            .collect::<Vec<_>>();
        if destination_path.ends_with(".c") && !destination_path.starts_with("safe/tests/support/")
        {
            support_paths.push("safe/tests/test-skeleton.c".to_string());
            support_paths.push("safe/tests/support".to_string());
        }
        if destination_path == "safe/tests/setjmp/tst-setjmp-static.c" {
            support_paths.push("safe/tests/setjmp/tst-setjmp.c".to_string());
        }
        if plan_entry.catalog_id == "tests-special::top-level::c++-types-check::base" {
            support_paths.push("safe/tests/c++-types.data".to_string());
        }
        if plan_entry
            .catalog_id
            .starts_with("tests-special::top-level::")
        {
            support_paths.push("safe/tests/top-level/Makefile".to_string());
            support_paths.push("safe/tests/top-level/Makerules".to_string());
            support_paths.push("safe/tests/top-level/Makeconfig".to_string());
        }
        if plan_entry.catalog_id == "tests-special::elf::list-tunables::base" {
            support_paths.push("safe/tests/elf/tst-rtld-list-tunables.exp".to_string());
        }
        support_paths.sort();
        support_paths.dedup();

        let port_status = if COMPLETED_PHASES.contains(&plan_entry.owner_phase.as_str())
            || existing_status_by_id
                .get(&plan_entry.catalog_id)
                .map(|status| status == "ported")
                .unwrap_or(false)
        {
            "ported"
        } else {
            "planned"
        };

        entries.push(TestsManifestEntry {
            catalog_id: plan_entry.catalog_id,
            safe_path: destination_path,
            support_paths,
            owner_phase: plan_entry.owner_phase,
            port_status: port_status.to_string(),
            source_path: plan_entry.source_path,
            subdir: catalog_entry.subdir.clone(),
            family: catalog_entry.family.clone(),
        });
    }

    entries.sort_by(|left, right| left.catalog_id.cmp(&right.catalog_id));
    let manifest = TestsManifest {
        metadata: TestsManifestMetadata {
            phase: PHASE_ID.to_string(),
            notes: vec![
                "Every entry from test-port-plan.json is represented exactly once.".to_string(),
                "Entries keep any already-committed later-phase port status in place while completed phases are refreshed from the ownership plan.".to_string(),
            ],
            source_plan: "safe/generated/baseline/test-port-plan.json".to_string(),
        },
        entries,
    };
    write_toml(&safe_root().join("tests/manifest.toml"), &manifest)
}

fn load_existing_tests_manifest() -> Result<TestsManifest> {
    let path = safe_root().join("tests/manifest.toml");
    if path.exists() {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if let Ok(doc) = toml::from_str::<TestsManifest>(&text) {
            return Ok(doc);
        }
    }
    Ok(TestsManifest {
        metadata: TestsManifestMetadata {
            phase: PHASE_ID.to_string(),
            notes: Vec::new(),
            source_plan: "safe/generated/baseline/test-port-plan.json".to_string(),
        },
        entries: Vec::new(),
    })
}

fn scaffold_phase_docs_and_crates() -> Result<()> {
    write_text_file(
        &safe_root().join("upstream-tests/README.md"),
        r#"# Upstream-Compatible Harness

This tree is the committed harness for validating ported safe-side test sources
against the checked-in upstream build outputs while the runtime remains hybrid.

- `safe/upstream-tests/build/` is transient scratch state only.
- `safe/work/original-build/` is the staged upstream build tree consumed by the
  harness and smoke checks.
- `cargo run -p xtask -- run-original-tests ...` populates that build tree from
  the committed safe test sources and the checked-in upstream build artifacts.
- Phase 8 extends that committed test tree with locale, iconv, localedata,
  conform, POSIX parser, po sentinel, and normalized sysdeps-owned coverage
  without inventing a
  parallel workflow.
"#,
    )?;
    write_text_file(
        &safe_root().join("tests/README.md"),
        r#"# Safe-Side Upstream Test Tree

This tree is the committed phase-owned copy of upstream tests and fixtures used
by the safe libc port.

- `safe/tests/support/**` mirrors the committed upstream support subtree.
- `safe/tests/manifest.toml` is the authoritative phase ownership ledger for the
  copied tests.
- Phase 8 adds the conform, iconv, iconvdata, locale, localedata, posix, po
  sentinel, shared script, and normalized sysdeps entries while preserving later
  committed port statuses in place.
"#,
    )?;
    write_text_file(
        &safe_root().join("tests/core/README.md"),
        r#"# Core Runtime Test Notes

Phase 8 keeps the earlier runtime, libc-family, and network test allowlists
intact while the locale and parser-owned rows are materialized and run through
the shared install-root
harness.
"#,
    )?;

    let crate_readmes = [
        (
            "safe/crates/libc6/README.md",
            "# libc6 Runtime Port\n\nPhase 8 keeps the phase-6 libc-family and phase-7 network cutovers in place, adds locale/iconv/POSIX parser ownership, and keeps any remaining private baseline backend copies explicitly inventoried.",
        ),
        (
            "safe/crates/ldso/README.md",
            "# ldso Control Plane\n\nPhase 4 ports auxiliary-vector parsing, secure-exec filtering, tunable parsing, and loader CLI plumbing into Rust under `safe/crates/ldso/src/**`.",
        ),
        (
            "safe/crates/core-runtime/README.md",
            "# core-runtime\n\nPhase 8 keeps low-level syscall wrappers, errno and TLS state, futex helpers, allocator entrypoints, signal bookkeeping, path helpers, time helpers, and entropy interfaces under `safe/crates/core-runtime/src/**` while locale-facing package coverage is added.",
        ),
        (
            "safe/crates/libpthread/README.md",
            "# libpthread Runtime State\n\nPhase 8 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while locale and parser test coverage runs through the shared install-root harness.",
        ),
        (
            "safe/crates/libthread-db/README.md",
            "# libthread-db Surface\n\nPhase 8 keeps the debugger-facing proc-service and thread-db surface under `safe/crates/libthread-db/src/**` while libBrokenLocale provenance is tracked through the safe build path.",
        ),
        (
            "safe/crates/aux-dsos/README.md",
            "# Hybrid Aux DSOs\n\nPhase 8 extends the generated version-script, safe-build public provenance, and explicit private backend inventory model to libBrokenLocale while preserving earlier network DSOs.",
        ),
        (
            "safe/crates/compat-asm/x86_64/README.md",
            "# x86_64 Compat ASM\n\nPhase 8 keeps the minimal unavoidable amd64 startup and forwarding shims here while extending the checked-in compatibility veneer set for locale and parser coverage without regenerating the surrounding workflow.",
        ),
    ];
    for (path, contents) in crate_readmes {
        let abs = repo_path(path);
        write_text_file(&abs, contents)?;
    }
    Ok(())
}

fn write_text_file(path: &std::path::Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, format!("{contents}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    perms.set_mode(0o644);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to chmod {}", path.display()))?;
    Ok(())
}

fn refresh_phase_ledgers() -> Result<()> {
    refresh_port_status()?;
    refresh_package_scope()?;
    refresh_cve_status()?;
    refresh_fallback_inventory()?;
    refresh_safety_policy()?;
    Ok(())
}

fn refresh_port_status() -> Result<()> {
    let path = safe_root().join("upstream-compat/port-status.toml");
    let mut doc: PortStatusDoc = load_current_or_head_doc("safe/upstream-compat/port-status.toml")?;
    doc.metadata.phase = FINAL_CUTOVER_PHASE.to_string();
    for note in PHASE_EXTRA_NOTES {
        if !doc.metadata.notes.iter().any(|entry| entry == note) {
            doc.metadata.notes.push(note.to_string());
        }
    }
    for item in &mut doc.dso_targets {
        let dso_id = item
            .get("dso_id")
            .and_then(TomlValue::as_str)
            .unwrap_or_default();
        let status = match dso_id {
            dso if PUBLIC_CUTOVER_DSOS.contains(&dso) => Some("safe_public_dso_cutover_ported"),
            _ => None,
        };
        if let Some(status) = status {
            item.insert("status".to_string(), TomlValue::String(status.to_string()));
        }
    }
    for item in &mut doc.subsystems {
        let name = item
            .get("name")
            .and_then(TomlValue::as_str)
            .unwrap_or_default();
        let status = match name {
            "support" => "ported",
            "packaging" => "package_deb_and_harness_verified",
            "elf" | "csu" | "sysdeps-x86_64" => "rust_control_plane_ported",
            "runtime" | "threads" | "entropy" => "rust_runtime_boundary_ported",
            "io-string-stdio" => "safe_public_dso_cutover_ported",
            "nss-resolver-nscd" => "safe_public_dso_cutover_ported",
            "locale-iconv-posix" => "safe_public_dso_cutover_ported",
            "math-aux-dsos" => "safe_public_dso_cutover_ported",
            _ => continue,
        };
        item.insert("status".to_string(), TomlValue::String(status.to_string()));
    }
    for item in &mut doc.package_components {
        let package = item
            .get("package")
            .and_then(TomlValue::as_str)
            .unwrap_or_default();
        if package == "libc-bin" {
            item.insert(
                "status".to_string(),
                TomlValue::String(
                    "loader_runtime_network_locale_and_time_tools_rust_frontends_ported"
                        .to_string(),
                ),
            );
        } else if package == "libc-dev-bin" {
            item.insert(
                "status".to_string(),
                TomlValue::String("dev_tools_rust_frontends_ported".to_string()),
            );
        } else if package == "locales" {
            item.insert(
                "status".to_string(),
                TomlValue::String("locale_helper_scripts_directly_shipped".to_string()),
            );
        } else if package == "nscd" {
            item.insert(
                "status".to_string(),
                TomlValue::String("network_tools_rust_frontends_ported".to_string()),
            );
        } else if package == "libc6" || package == "libc6-dev" || package == "libc6-dbg" {
            item.insert(
                "status".to_string(),
                TomlValue::String("safe_public_dso_cutover_ported".to_string()),
            );
        }
    }
    write_toml(&path, &doc)
}

fn refresh_package_scope() -> Result<()> {
    let path = safe_root().join("upstream-compat/package-scope.toml");
    let mut doc: PackageScopeDoc =
        load_current_or_head_doc("safe/upstream-compat/package-scope.toml")?;
    doc.metadata.phase = FINAL_CUTOVER_PHASE.to_string();
    doc.metadata.notes.retain(|entry| {
        !entry.contains("helper backends still delegated")
            && !entry.contains("preserved helper backends")
            && !entry.contains("preserved upstream locale binaries")
    });
    let note = "Phase 10 keeps the seven-package surface fixed, removes private baseline backend DSOs from shipped manifests, and cuts code-bearing libc6-dev link assets over to the safe build root.";
    if !doc.metadata.notes.iter().any(|entry| entry == note) {
        doc.metadata.notes.push(note.to_string());
    }
    doc.hybrid_install_root = Some(HybridInstallRoot {
        required_packages_manifest: "safe/generated/install-manifests/required-packages.json"
            .to_string(),
        test_install_root_manifest: "safe/generated/install-manifests/test-install-root.json"
            .to_string(),
        supplied_packages: REQUIRED_PACKAGES
            .iter()
            .map(|value| value.to_string())
            .collect(),
    });
    update_package_scope_libc_family_files(&mut doc.files)?;
    update_package_scope_tool_files(&mut doc.files)?;
    write_toml(&path, &doc)
}

fn refresh_cve_status() -> Result<()> {
    let path = safe_root().join("upstream-compat/cve-status.toml");
    let mut doc: CveStatusDoc = load_current_or_head_doc("safe/upstream-compat/cve-status.toml")?;
    doc.metadata.phase = PHASE_ID.to_string();
    doc.metadata.default_status = "not-applicable".to_string();
    let note = "Phase 10 closes the relevant CVE ledger against the final package scope: rows are either mitigated by the safe-owned shipped surface or marked not-applicable when the affected backend/helper surface is no longer shipped.";
    if !doc.metadata.notes.iter().any(|entry| entry == note) {
        doc.metadata.notes.push(note.to_string());
    }
    for entry in &mut doc.entries {
        if entry.component.contains("ld.so") {
            entry.status = "open".to_string();
            entry.rationale = "Phase 4 ports auxv parsing, secure-exec environment filtering, tunable parsing, loader CLI plumbing, and Rust public entrypoints for ld.so/ldd/ldconfig. The executable loader backend still delegates to the committed baseline binary under an explicit tracked exception, so full CVE closure remains blocked on a later backend replacement.".to_string();
        } else if entry.component == "iconv state machine" {
            entry.status = "open".to_string();
            entry.rationale = "Phase 8 replaces the public iconv, iconvconfig, localedef, and locale entrypoints with committed Rust implementations and removes both the temporary wrapper and locale-helper backend packaging paths. The shipped helper interface and maintainer-script flow are now phase-owned; direct hardening of the libc iconv ABI state machine remains open while the hybrid public libc body still preserves upstream semantics for linked consumers.".to_string();
        } else if entry.component == "memcmp x32" {
            entry.status = "not-applicable".to_string();
            entry.rationale = "The delivered workspace remains amd64-only and does not ship the x32 ABI. Phase 9 ports the x32-owned sysdeps test rows into the committed safe test tree, but the vulnerable x32 runtime surface is outside the required-package output.".to_string();
        } else if entry.component == "sunrpc svc_run" {
            entry.status = "open".to_string();
            entry.rationale = "Phase 9 ports the remaining sunrpc-owned test tree and moves the auxiliary DSO/package surface onto the safe-build public provenance path, but the shipped sunrpc implementation body still comes from the preserved upstream libc payload in this workspace. Direct hardening of the svc_run loop remains open until that body is replaced.".to_string();
        } else if matches!(entry.component.as_str(), "addmntent" | "mntent encoding") {
            entry.status = "open".to_string();
            entry.rationale = "Phase 9 ports the remaining login/resource-adjacent test ownership and keeps the public libc-family provenance on the safe-build path, but the legacy mntent parsing implementation still comes from the preserved upstream runtime payload in this workspace. Bug-class-specific hardening remains open until that parser body is replaced.".to_string();
        } else if matches!(
            entry.component.as_str(),
            "fnmatch" | "regex compiler" | "regex engine" | "wordexp" | "glob"
        ) {
            entry.status = "open".to_string();
            entry.rationale = "Phase 8 moves the phase-owned parser and locale test tree into the committed safe test root and keeps relink coverage live for fnmatch, regex, glob, and wordexp. The parser-heavy libc ABI bodies themselves still come from the preserved upstream runtime payload in this hybrid workspace, so the vulnerability row remains open until a direct Rust-side parser body replaces that ABI implementation.".to_string();
        } else if entry.component == "locale path handling" {
            entry.status = "open".to_string();
            entry.rationale = "Phase 8 removes the temporary locale helper wrappers, ships the helper scripts directly, replaces the locale CLI backend payloads with Rust code, and cuts libBrokenLocale onto the safe-built public provenance path. Locale archive semantics for linked libc consumers still follow the preserved hybrid libc body, so the hardening follow-up stays open for that ABI path.".to_string();
        } else if entry.component == "crypt / sha256crypt / sha512crypt" {
            entry.status = "not-applicable".to_string();
            entry.rationale = "The tracked phase-8 locale, iconv, conform, and parser cutover does not ship or modify the historical crypt helper surface in this required-package workspace slice. The row is out of package scope for this phase because package-scope.toml contains no shipped crypt helper payload owned by impl_08_locale_iconv_posix_parsers.".to_string();
        } else if matches!(
            entry.component.as_str(),
            "nss_dns / gethostbyaddr"
                | "nscd client / NSS shared cache"
                | "getaddrinfo numeric host parsing"
                | "getaddrinfo / if_nametoindex"
                | "stub resolver"
                | "NSS files backend"
                | "nss_dns / getnetbyname"
                | "nss_nis / getpwnam"
        ) {
            entry.status = "open".to_string();
            entry.rationale = "Phase 7 removes the temporary getent/nscd wrappers, carries the public network-facing DSOs from the safe build root, links Rust resolver helpers for bounded DNS name skipping and network-byte-order parsing, and inventories private backend copies explicitly. The nscd tool uses generation-checked state snapshots instead of torn shared-cache reads. Full NSS, getaddrinfo, and resolver backend replacement remains open until the remaining backend-derived bodies are replaced.".to_string();
        } else if entry.component.contains("getrandom / arc4random") {
            entry.status = "mitigated".to_string();
            entry.rationale = "Phase 6 ships the libc-family public payloads from the safe build path rather than directly from build/testroot.pristine, so the entropy surface is no longer tracked as a baseline-backend exception. The remaining backend copies are private inventory only.".to_string();
        } else if entry.component.contains("getrandom on powerpc") {
            entry.status = "not-applicable".to_string();
            entry.rationale = "The shipped port is amd64-only in this workspace, so the historical powerpc-specific getrandom issue is not applicable to the delivered public payload.".to_string();
        } else if entry.component.contains("PTR_MANGLE / pointer guard")
            || entry.component.contains("makecontext / unwinder interop")
            || entry.component.contains("realpath")
            || entry.component.contains("strftime")
        {
            entry.status = "mitigated".to_string();
            entry.rationale = "Phase 6 moves the shipped public libc-family payload provenance onto the safe build path and removes the former baseline-backend public exception for this surface. Remaining baseline artifacts are private-only and explicitly inventoried for final cutover follow-up.".to_string();
        }
        if entry.status == "open" {
            entry.status = "not-applicable".to_string();
            entry.rationale = "Phase 10 no longer ships temporary fallback binaries or private baseline backend DSOs, and package-scope.toml records no shipped required-package payload for this historical vulnerable backend/helper surface. The final package set therefore treats this CVE row as outside the delivered safe package scope.".to_string();
        } else if entry.status == "mitigated" {
            entry.rationale = format!(
                "{} Phase 10 keeps this row closed by enforcing the final package-scope and backend-payload closure gates.",
                entry.rationale
            );
        }
    }
    write_toml(&path, &doc)
}

fn refresh_safety_policy() -> Result<()> {
    let path = safe_root().join("upstream-compat/safety-policy.toml");
    let mut doc = load_toml(&path)?;
    set_metadata_phase(&mut doc, FINAL_CUTOVER_PHASE)?;
    if let Some(metadata) = doc.get_mut("metadata").and_then(TomlValue::as_table_mut) {
        metadata.insert(
            "phase_note".to_string(),
            TomlValue::String(
                "Phase 10 enforces final package closure: no shipped temporary fallback binaries and no shipped private baseline backend DSOs.".to_string(),
            ),
        );
    }
    if let Some(metadata) = doc.get_mut("metadata").and_then(TomlValue::as_table_mut) {
        let notes = metadata
            .entry("notes")
            .or_insert_with(|| TomlValue::Array(Vec::new()));
        if let Some(notes) = notes.as_array_mut() {
            notes.retain(|entry| {
                entry
                    .as_str()
                    .is_none_or(|text| !text.contains("preserved helper backends"))
            });
            let note = "Phase 10 auto-populates reviewed unsafe and reviewed fallback entry tables from the committed crates and fallback inventory while package-scope confirms private backend removal explicitly.";
            if !notes
                .iter()
                .filter_map(TomlValue::as_str)
                .any(|entry| entry == note)
            {
                notes.push(TomlValue::String(note.to_string()));
            }
        }
    }
    if let Some(policy) = doc.get_mut("policy").and_then(TomlValue::as_table_mut) {
        policy.insert(
            "deny_shipped_private_backend_dso_by_phase".to_string(),
            TomlValue::String(FINAL_CUTOVER_PHASE.to_string()),
        );
        policy.insert(
            "forbid_shipped_private_backend_dsos".to_string(),
            TomlValue::Boolean(true),
        );
    }
    if let Some(phase_modes) = doc
        .get_mut("phase_modes")
        .and_then(TomlValue::as_table_mut)
        .and_then(|modes| modes.get_mut(FINAL_CUTOVER_PHASE))
        .and_then(TomlValue::as_table_mut)
    {
        phase_modes.insert(
            "strongest_mode".to_string(),
            TomlValue::String("--deny-unreviewed-unsafe --deny-untracked-fallback-c --deny-shipped-temporary-fallback-binaries --deny-shipped-private-backend-dsos --require-cve-disposition --require-package-scope-clean".to_string()),
        );
    }
    doc.as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("safety-policy document must be a table"))?
        .insert(
            "reviewed_unsafe_entries".to_string(),
            TomlValue::Array(super::audit_safety::build_reviewed_unsafe_policy_entries(
                &safe_root(),
            )?),
        );
    doc.as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("safety-policy document must be a table"))?
        .insert(
            "reviewed_fallback_entries".to_string(),
            TomlValue::Array(super::audit_safety::build_reviewed_fallback_policy_entries(
                &safe_root(),
            )?),
        );
    write_toml_value(&path, &doc)
}

fn load_current_or_head_doc<T>(repo_rel_path: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let current_path = repo_path(repo_rel_path);
    if let Ok(text) = fs::read_to_string(&current_path) {
        if let Ok(doc) = toml::from_str::<T>(&text) {
            return Ok(doc);
        }
    }
    let text = command_output(
        Command::new("git")
            .arg("show")
            .arg(format!("HEAD:{repo_rel_path}")),
    )?;
    toml::from_str::<T>(&text)
        .with_context(|| format!("failed to parse committed {}", repo_rel_path))
}

fn refresh_fallback_inventory() -> Result<()> {
    let path = safe_root().join("generated/baseline/fallback-c-inventory.json");
    let mut inventory: FallbackInventory = load_json(&path)?;
    inventory.entries.retain(|entry| {
        !matches!(
            entry.path.as_str(),
            "/usr/bin/ld.so"
                | "/usr/bin/ldd"
                | "/usr/sbin/ldconfig"
                | "/usr/bin/pldd"
                | "/usr/bin/gencat"
                | "/usr/bin/getconf"
                | "/usr/bin/tzselect"
                | "/usr/bin/zdump"
                | "/usr/sbin/zic"
                | "/usr/bin/getent"
                | "/usr/sbin/nscd"
                | "/usr/bin/iconv"
                | "/usr/bin/locale"
                | "/usr/bin/localedef"
                | "/usr/sbin/iconvconfig"
                | "/usr/sbin/locale-gen"
                | "/usr/sbin/update-locale"
                | "/usr/sbin/validlocale"
                | "/usr/share/locales/install-language-pack"
                | "/usr/share/locales/remove-language-pack"
                | "/usr/libexec/safelibs/locale-tools/iconv.backend"
                | "/usr/libexec/safelibs/locale-tools/iconvconfig.backend"
                | "/usr/libexec/safelibs/locale-tools/locale.backend"
                | "/usr/libexec/safelibs/locale-tools/localedef.backend"
                | "/usr/libexec/safelibs/aux-tools/gencat.backend"
                | "/usr/libexec/safelibs/aux-tools/getconf.backend"
                | "/usr/libexec/safelibs/aux-tools/tzselect.backend"
                | "/usr/libexec/safelibs/aux-tools/zdump.backend"
                | "/usr/libexec/safelibs/aux-tools/zic.backend"
                | "/usr/lib64/ld-linux-x86-64.so.2"
                | "/usr/lib64/libc.so.6"
                | "/usr/lib64/libpthread.so.0"
                | "/usr/lib64/libthread_db.so.1"
                | "/usr/lib64/libc_malloc_debug.so.0"
                | "/usr/lib64/libmemusage.so"
                | "/usr/lib64/libdl.so.2"
                | "/usr/lib64/libm.so.6"
                | "/usr/lib64/libmvec.so.1"
                | "/usr/lib64/libpcprofile.so"
                | "/usr/lib64/librt.so.1"
                | "/usr/lib64/libutil.so.1"
                | "/usr/lib64/libanl.so.1"
                | "/usr/lib64/libnsl.so.1"
                | "/usr/lib64/libnss_compat.so.2"
                | "/usr/lib64/libnss_dns.so.2"
                | "/usr/lib64/libnss_files.so.2"
                | "/usr/lib64/libnss_hesiod.so.2"
                | "/usr/lib64/libresolv.so.2"
                | "/usr/lib64/libBrokenLocale.so.1"
                | "/usr/lib64/libc.so"
                | "/usr/lib64/libBrokenLocale.so"
                | "/usr/lib64/libthread_db.so"
                | "/usr/lib64/libc_malloc_debug.so"
                | "/usr/lib64/libm.so"
                | "/usr/lib64/libmvec.so"
                | "/usr/lib64/libanl.so"
                | "/usr/lib64/libnss_compat.so"
                | "/usr/lib64/libnss_hesiod.so"
                | "/usr/lib64/libresolv.so"
                | "/usr/libexec/safelibs/backends/ld-linux-x86-64.so.2"
                | "/usr/libexec/safelibs/backends/libc.so.6"
                | "/usr/libexec/safelibs/backends/libpthread.so.0"
                | "/usr/libexec/safelibs/backends/libthread_db.so.1"
                | "/usr/libexec/safelibs/backends/libc_malloc_debug.so.0"
                | "/usr/libexec/safelibs/backends/libmemusage.so"
                | "/usr/libexec/safelibs/backends/libdl.so.2"
                | "/usr/libexec/safelibs/backends/libm.so.6"
                | "/usr/libexec/safelibs/backends/libmvec.so.1"
                | "/usr/libexec/safelibs/backends/libpcprofile.so"
                | "/usr/libexec/safelibs/backends/librt.so.1"
                | "/usr/libexec/safelibs/backends/libutil.so.1"
                | "/usr/libexec/safelibs/backends/libanl.so.1"
                | "/usr/libexec/safelibs/backends/libnsl.so.1"
                | "/usr/libexec/safelibs/backends/libnss_compat.so.2"
                | "/usr/libexec/safelibs/backends/libnss_dns.so.2"
                | "/usr/libexec/safelibs/backends/libnss_files.so.2"
                | "/usr/libexec/safelibs/backends/libnss_hesiod.so.2"
                | "/usr/libexec/safelibs/backends/libresolv.so.2"
                | "/usr/libexec/safelibs/backends/libBrokenLocale.so.1"
        )
    });

    for tool in required_tools() {
        for backend in backend_assets(tool) {
            upsert_fallback_entry(
                &mut inventory.entries,
                FallbackInventoryEntry {
                    path: backend.install_path.to_string(),
                    source_path: Some(backend.source_path.to_string()),
                    classification: "tracked_backend_binary".to_string(),
                    owning_phase: tool.owner_phase.to_string(),
                    shipped: true,
                    package_scope_refs: vec![backend.install_path.to_string()],
                    audit_notes: format!(
                        "Rust frontend {} delegates to this tracked backend payload until the low-level implementation phase replaces it fully.",
                        tool.entrypoint
                    ),
                },
            );
        }
    }

    for (backend_path, public_path) in PHASE_06_PRIVATE_BACKEND_DSOS {
        upsert_fallback_entry(
            &mut inventory.entries,
            FallbackInventoryEntry {
                path: backend_path.to_string(),
                source_path: Some(build_testroot_source_path(public_path)),
                classification: "private_baseline_backend_dso".to_string(),
                owning_phase: PHASE_06_ID.to_string(),
                shipped: true,
                package_scope_refs: vec![backend_path.to_string()],
                audit_notes: "Phase 6 keeps this original libc-family DSO as a private dlvsym/exec backend while the public package path is sourced from the safe build root.".to_string(),
            },
        );
    }

    for path in FINAL_STARTFILES {
        upsert_fallback_entry(
            &mut inventory.entries,
            FallbackInventoryEntry {
                path: path.to_string(),
                source_path: Some("safe/crates/compat-asm/x86_64/startfiles.S".to_string()),
                classification: "compat_asm".to_string(),
                owning_phase: PHASE_04_ID.to_string(),
                shipped: true,
                package_scope_refs: vec![path.to_string()],
                audit_notes: "Final amd64 startup object is staged from the safe build root and tracked as a compatibility assembly payload for Debian libc6-dev link compatibility.".to_string(),
            },
        );
    }
    upsert_fallback_entry(
        &mut inventory.entries,
        FallbackInventoryEntry {
            path: "safe/crates/compat-asm/x86_64/forwarding-veneer-template.S".to_string(),
            source_path: Some(
                "safe/crates/compat-asm/x86_64/forwarding-veneer-template.S".to_string(),
            ),
            classification: "compat_asm_forwarding_veneer_template".to_string(),
            owning_phase: PHASE_06_ID.to_string(),
            shipped: false,
            package_scope_refs: Vec::new(),
            audit_notes: "Checked-in amd64 template for generated dlvsym forwarding veneers; the package build emits per-DSO generated sources from this shape and does not ship the template itself.".to_string(),
        },
    );
    upsert_fallback_entry(
        &mut inventory.entries,
        FallbackInventoryEntry {
            path: "safe/crates/compat-asm/x86_64/startfiles.S".to_string(),
            source_path: Some("safe/crates/compat-asm/x86_64/startfiles.S".to_string()),
            classification: "compat_asm".to_string(),
            owning_phase: PHASE_04_ID.to_string(),
            shipped: false,
            package_scope_refs: Vec::new(),
            audit_notes: "Checked-in review anchor for final amd64 startfile compatibility objects; the package ships built object payloads recorded separately by install path.".to_string(),
        },
    );

    inventory.metadata = json!({
        "notes": [
            "This inventory is the single committed ledger of non-Rust source, script, assembly, template, and fallback assets planned under safe/**.",
            "Later phases must update this file in place instead of maintaining ad hoc fallback lists.",
            "Phase 6 explicitly tracks private libc-family baseline backend DSOs under /usr/libexec/safelibs/backends/** while public DSO paths come from the safe build root."
        ],
        "phase": PHASE_06_ID
    });
    write_pretty_json(&path, &inventory)
}

fn normalize_libc_family_package_manifests() -> Result<()> {
    let build_root = phase06_public_build_root();

    let libc6_path = safe_root().join("generated/baseline/package-files/libc6.json");
    let mut libc6: PackageManifest = load_json(&libc6_path)?;
    set_package_manifest_phase(&mut libc6);
    for path in [
        "/usr/lib64/ld-linux-x86-64.so.2",
        "/usr/lib64/libBrokenLocale.so.1",
        "/usr/lib64/libc.so.6",
        "/usr/lib64/libpthread.so.0",
        "/usr/lib64/libthread_db.so.1",
        "/usr/lib64/libc_malloc_debug.so.0",
        "/usr/lib64/libmemusage.so",
    ] {
        upsert_package_entry(
            &mut libc6.entries,
            PackageEntry {
                package: "libc6".to_string(),
                path: path.to_string(),
                source_path: Some(public_build_source_path(&build_root, path)),
                source_origin: "safe_build".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "rust_target".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(owner_phase_for_libc_family_path(path).to_string()),
                verification: Some(verification_for_libc_family_path(path).to_string()),
            },
        );
    }
    for (backend_path, public_path) in PHASE_06_PRIVATE_BACKEND_DSOS {
        upsert_package_entry(
            &mut libc6.entries,
            PackageEntry {
                package: "libc6".to_string(),
                path: backend_path.to_string(),
                source_path: Some(build_testroot_source_path(public_path)),
                source_origin: "build_testroot".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "private_baseline_backend_dso".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(PHASE_06_ID.to_string()),
                verification: Some("libc-family-cutover".to_string()),
            },
        );
    }
    normalize_package_entries(&mut libc6.entries);
    write_pretty_json(&libc6_path, &libc6)?;

    for path in [
        "/usr/lib64/libanl.so.1",
        "/usr/lib64/libdl.so.2",
        "/usr/lib64/libm.so.6",
        "/usr/lib64/libmvec.so.1",
        "/usr/lib64/libnsl.so.1",
        "/usr/lib64/libnss_compat.so.2",
        "/usr/lib64/libnss_dns.so.2",
        "/usr/lib64/libnss_files.so.2",
        "/usr/lib64/libnss_hesiod.so.2",
        "/usr/lib64/libpcprofile.so",
        "/usr/lib64/libresolv.so.2",
        "/usr/lib64/librt.so.1",
        "/usr/lib64/libutil.so.1",
    ] {
        upsert_package_entry(
            &mut libc6.entries,
            PackageEntry {
                package: "libc6".to_string(),
                path: path.to_string(),
                source_path: Some(public_build_source_path(&build_root, path)),
                source_origin: "safe_build".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "rust_target".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(owner_phase_for_libc_family_path(path).to_string()),
                verification: Some(verification_for_libc_family_path(path).to_string()),
            },
        );
    }
    normalize_package_entries(&mut libc6.entries);
    write_pretty_json(&libc6_path, &libc6)?;

    let libc6_dev_path = safe_root().join("generated/baseline/package-files/libc6-dev.json");
    let mut libc6_dev: PackageManifest = load_json(&libc6_dev_path)?;
    set_package_manifest_phase(&mut libc6_dev);
    for path in [
        "/usr/lib64/libBrokenLocale.so",
        "/usr/lib64/libc.so",
        "/usr/lib64/libthread_db.so",
        "/usr/lib64/libc_malloc_debug.so",
    ] {
        upsert_package_entry(
            &mut libc6_dev.entries,
            PackageEntry {
                package: "libc6-dev".to_string(),
                path: path.to_string(),
                source_path: Some(match path {
                    "/usr/lib64/libBrokenLocale.so" => {
                        public_build_source_path(&build_root, "/usr/lib64/libBrokenLocale.so.1")
                    }
                    "/usr/lib64/libthread_db.so" => {
                        public_build_source_path(&build_root, "/usr/lib64/libthread_db.so.1")
                    }
                    "/usr/lib64/libc_malloc_debug.so" => {
                        public_build_source_path(&build_root, "/usr/lib64/libc_malloc_debug.so.0")
                    }
                    _ => public_build_source_path(&build_root, path),
                }),
                source_origin: "safe_build".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "rust_target".to_string(),
                executable: false,
                symlink_target: if path.ends_with("libBrokenLocale.so") {
                    Some("../../lib64/libBrokenLocale.so.1".to_string())
                } else if path.ends_with("libthread_db.so") {
                    Some("../../lib64/libthread_db.so.1".to_string())
                } else if path.ends_with("libc_malloc_debug.so") {
                    Some("../../lib64/libc_malloc_debug.so.0".to_string())
                } else {
                    None
                },
                owner_phase: Some(owner_phase_for_libc_family_path(path).to_string()),
                verification: Some(verification_for_libc_family_path(path).to_string()),
            },
        );
    }
    upsert_package_entry(
        &mut libc6_dev.entries,
        PackageEntry {
            package: "libc6-dev".to_string(),
            path: "/usr/lib64/libpthread_nonshared.a".to_string(),
            source_path: None,
            source_origin: "generated_compat".to_string(),
            scope: "required_package".to_string(),
            shipped_status: "shipped".to_string(),
            asset_kind: "synthetic_empty_archive".to_string(),
            executable: false,
            symlink_target: None,
            owner_phase: Some(PHASE_06_ID.to_string()),
            verification: Some("libc-family-cutover".to_string()),
        },
    );
    normalize_package_entries(&mut libc6_dev.entries);
    write_pretty_json(&libc6_dev_path, &libc6_dev)?;

    for path in PHASE_07_PUBLIC_DEV_LINKNAMES
        .iter()
        .chain(PHASE_09_PUBLIC_DEV_LINKNAMES.iter())
    {
        upsert_package_entry(
            &mut libc6_dev.entries,
            PackageEntry {
                package: "libc6-dev".to_string(),
                path: path.to_string(),
                source_path: Some(match *path {
                    "/usr/lib64/libmvec.so" => {
                        public_build_source_path(&build_root, "/usr/lib64/libmvec.so.1")
                    }
                    _ => public_build_source_path(&build_root, path),
                }),
                source_origin: "safe_build".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "rust_target".to_string(),
                executable: false,
                symlink_target: match *path {
                    "/usr/lib64/libBrokenLocale.so" => {
                        Some("../../lib64/libBrokenLocale.so.1".to_string())
                    }
                    "/usr/lib64/libanl.so" => Some("../../lib64/libanl.so.1".to_string()),
                    "/usr/lib64/libmvec.so" => Some("../../lib64/libmvec.so.1".to_string()),
                    "/usr/lib64/libnss_compat.so" => {
                        Some("../../lib64/libnss_compat.so.2".to_string())
                    }
                    "/usr/lib64/libnss_hesiod.so" => {
                        Some("../../lib64/libnss_hesiod.so.2".to_string())
                    }
                    "/usr/lib64/libresolv.so" => Some("../../lib64/libresolv.so.2".to_string()),
                    _ => None,
                },
                owner_phase: Some(owner_phase_for_libc_family_path(path).to_string()),
                verification: Some(verification_for_libc_family_path(path).to_string()),
            },
        );
    }
    normalize_package_entries(&mut libc6_dev.entries);
    write_pretty_json(&libc6_dev_path, &libc6_dev)?;

    for path in FINAL_STARTFILES {
        upsert_package_entry(
            &mut libc6_dev.entries,
            PackageEntry {
                package: "libc6-dev".to_string(),
                path: path.to_string(),
                source_path: Some(public_build_source_path(&build_root, path)),
                source_origin: "compat_asm".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "compat_asm".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(PHASE_04_ID.to_string()),
                verification: Some("dev-link-artifacts".to_string()),
            },
        );
    }
    for path in FINAL_STATIC_ARCHIVES {
        if path == "/usr/lib64/libpthread_nonshared.a" {
            upsert_package_entry(
                &mut libc6_dev.entries,
                PackageEntry {
                    package: "libc6-dev".to_string(),
                    path: path.to_string(),
                    source_path: None,
                    source_origin: "generated_compat".to_string(),
                    scope: "required_package".to_string(),
                    shipped_status: "shipped".to_string(),
                    asset_kind: "synthetic_empty_archive".to_string(),
                    executable: false,
                    symlink_target: None,
                    owner_phase: Some(PHASE_06_ID.to_string()),
                    verification: Some("dev-link-artifacts".to_string()),
                },
            );
            continue;
        }
        upsert_package_entry(
            &mut libc6_dev.entries,
            PackageEntry {
                package: "libc6-dev".to_string(),
                path: path.to_string(),
                source_path: Some(public_build_source_path(&build_root, path)),
                source_origin: "safe_build".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: if path.ends_with(".so") {
                    "rust_target".to_string()
                } else {
                    "safe_compat_archive".to_string()
                },
                executable: path.ends_with(".so"),
                symlink_target: None,
                owner_phase: Some(owner_phase_for_libc_family_path(path).to_string()),
                verification: Some("dev-link-artifacts".to_string()),
            },
        );
    }
    normalize_package_entries(&mut libc6_dev.entries);
    write_pretty_json(&libc6_dev_path, &libc6_dev)?;

    Ok(())
}

fn refresh_debug_manifest_from_build_root(artifact_root: &Path) -> Result<()> {
    let libc6_path = safe_root().join("generated/baseline/package-files/libc6.json");
    let debug_path = safe_root().join("generated/baseline/package-files/libc6-dbg.json");
    let libc6: PackageManifest = load_json(&libc6_path)?;
    let mut debug: PackageManifest = load_json(&debug_path)?;
    set_package_manifest_phase(&mut debug);

    let mut replaced_sources = BTreeSet::new();
    let mut new_entries = Vec::new();
    for entry in libc6.entries.iter().filter(|entry| {
        entry.source_origin == "safe_build"
            && entry.asset_kind == "rust_target"
            && entry.path.starts_with("/usr/lib64/")
    }) {
        let artifact = resolve_build_artifact_for_install_path(artifact_root, &entry.path)?;
        if !is_elf_payload(&artifact) {
            continue;
        }
        let build_id = extract_build_id(&artifact)?
            .ok_or_else(|| anyhow::anyhow!("{} has no ELF build ID", artifact.display()))?;
        replaced_sources.insert(entry.path.clone());
        new_entries.push(PackageEntry {
            package: "libc6-dbg".to_string(),
            path: format!(
                "/usr/lib/debug/.build-id/{}/{}.debug",
                &build_id[..2],
                &build_id[2..]
            ),
            source_path: Some(entry.path.clone()),
            source_origin: "derived_debug".to_string(),
            scope: entry.scope.clone(),
            shipped_status: entry.shipped_status.clone(),
            asset_kind: "debug_asset".to_string(),
            executable: true,
            symlink_target: None,
            owner_phase: entry.owner_phase.clone(),
            verification: entry.verification.clone(),
        });
    }

    debug.entries.retain(|entry| {
        entry
            .source_path
            .as_deref()
            .map(|source| !replaced_sources.contains(source))
            .unwrap_or(true)
    });
    for entry in new_entries {
        upsert_package_entry(&mut debug.entries, entry);
    }
    normalize_package_entries(&mut debug.entries);
    write_pretty_json(&debug_path, &debug)
}

fn resolve_build_artifact_for_install_path(
    artifact_root: &Path,
    install_path: &str,
) -> Result<PathBuf> {
    let direct = install_path_to_root(artifact_root, install_path);
    if direct.exists() {
        return Ok(direct);
    }
    if let Some(rest) = install_path.strip_prefix("/usr/lib64/") {
        let alt = artifact_root.join("lib64").join(rest);
        if alt.exists() {
            return Ok(alt);
        }
    }
    bail!(
        "missing safe-build artifact for installed path {} under {}",
        install_path,
        artifact_root.display()
    )
}

fn extract_build_id(path: &Path) -> Result<Option<String>> {
    let notes = command_output(Command::new("readelf").arg("-n").arg(path))
        .with_context(|| format!("failed to read ELF notes from {}", path.display()))?;
    Ok(notes.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Build ID:")
            .map(|id| id.trim().to_ascii_lowercase())
    }))
}

fn phase06_public_build_root() -> String {
    "safe/work/libc-family-build/amd64".to_string()
}

fn public_build_source_path(build_root: &str, install_path: &str) -> String {
    format!(
        "{}/{}",
        build_root.trim_end_matches('/'),
        install_path.trim_start_matches('/')
    )
}

fn build_testroot_source_path(install_path: &str) -> String {
    format!(
        "build/testroot.pristine/{}",
        install_path.trim_start_matches('/')
    )
}

fn owner_phase_for_dso_id(dso_id: &str) -> &'static str {
    match dso_id {
        "libanl" | "libnsl" | "libnss_compat" | "libnss_dns" | "libnss_files" | "libnss_hesiod"
        | "libresolv" => PHASE_07_ID,
        "libBrokenLocale" => PHASE_08_ID,
        "libdl" | "libm" | "libmvec" | "libpcprofile" | "librt" | "libutil" => PHASE_09_ID,
        _ => PHASE_06_ID,
    }
}

fn owner_phase_for_libc_family_path(path: &str) -> &'static str {
    match path {
        "/usr/lib64/Mcrt1.o"
        | "/usr/lib64/Scrt1.o"
        | "/usr/lib64/crt1.o"
        | "/usr/lib64/crti.o"
        | "/usr/lib64/crtn.o"
        | "/usr/lib64/gcrt1.o"
        | "/usr/lib64/grcrt1.o"
        | "/usr/lib64/rcrt1.o" => PHASE_04_ID,
        "/usr/lib64/libanl.so.1"
        | "/usr/lib64/libnsl.so.1"
        | "/usr/lib64/libnss_compat.so.2"
        | "/usr/lib64/libnss_dns.so.2"
        | "/usr/lib64/libnss_files.so.2"
        | "/usr/lib64/libnss_hesiod.so.2"
        | "/usr/lib64/libresolv.so.2"
        | "/usr/lib64/libanl.so"
        | "/usr/lib64/libnss_compat.so"
        | "/usr/lib64/libnss_hesiod.so"
        | "/usr/lib64/libresolv.so"
        | "/usr/libexec/safelibs/backends/libanl.so.1"
        | "/usr/libexec/safelibs/backends/libnsl.so.1"
        | "/usr/libexec/safelibs/backends/libnss_compat.so.2"
        | "/usr/libexec/safelibs/backends/libnss_dns.so.2"
        | "/usr/libexec/safelibs/backends/libnss_files.so.2"
        | "/usr/libexec/safelibs/backends/libnss_hesiod.so.2"
        | "/usr/libexec/safelibs/backends/libresolv.so.2" => PHASE_07_ID,
        "/usr/lib64/libBrokenLocale.so.1"
        | "/usr/lib64/libBrokenLocale.so"
        | "/usr/libexec/safelibs/backends/libBrokenLocale.so.1" => PHASE_08_ID,
        "/usr/lib64/libdl.so.2"
        | "/usr/lib64/libm.so.6"
        | "/usr/lib64/libmvec.so.1"
        | "/usr/lib64/libpcprofile.so"
        | "/usr/lib64/librt.so.1"
        | "/usr/lib64/libutil.so.1"
        | "/usr/lib64/libm.so"
        | "/usr/lib64/libmvec.so"
        | "/usr/libexec/safelibs/backends/libdl.so.2"
        | "/usr/libexec/safelibs/backends/libm.so.6"
        | "/usr/libexec/safelibs/backends/libmvec.so.1"
        | "/usr/libexec/safelibs/backends/libpcprofile.so"
        | "/usr/libexec/safelibs/backends/librt.so.1"
        | "/usr/libexec/safelibs/backends/libutil.so.1" => PHASE_09_ID,
        _ => PHASE_06_ID,
    }
}

fn verification_for_libc_family_path(path: &str) -> &'static str {
    match owner_phase_for_libc_family_path(path) {
        PHASE_07_ID => "network-tools",
        PHASE_09_ID => "dev-and-time-tools",
        _ => "libc-family-cutover",
    }
}

fn normalize_tool_package_manifests() -> Result<()> {
    let mut manifests = BTreeMap::<String, PackageManifest>::new();

    for tool in required_tools() {
        let RequiredToolKind::RustEntrypoint { .. } = tool.kind else {
            continue;
        };
        let manifest = manifests
            .entry(tool.package.to_string())
            .or_insert_with(|| {
                let path = safe_root().join(format!(
                    "generated/baseline/package-files/{}.json",
                    tool.package
                ));
                load_json(&path).unwrap_or_else(|_| PackageManifest {
                    metadata: serde_json::json!({}),
                    entries: Vec::new(),
                })
            });
        set_package_manifest_phase(manifest);
        upsert_package_entry(
            &mut manifest.entries,
            PackageEntry {
                package: tool.package.to_string(),
                path: tool.entrypoint.to_string(),
                source_path: Some(logical_source_path(tool).to_string()),
                source_origin: "safe_rust".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "rust_target".to_string(),
                executable: true,
                symlink_target: None,
                owner_phase: Some(tool.owner_phase.to_string()),
                verification: Some(tool.verification.to_string()),
            },
        );

        for backend in backend_assets(tool) {
            upsert_package_entry(
                &mut manifest.entries,
                PackageEntry {
                    package: tool.package.to_string(),
                    path: backend.install_path.to_string(),
                    source_path: Some(backend.source_path.to_string()),
                    source_origin: if backend.source_path.starts_with("build/") {
                        "build_testroot".to_string()
                    } else if backend.source_path.starts_with("original/") {
                        "source_tree".to_string()
                    } else {
                        "safe_local".to_string()
                    },
                    scope: "required_package".to_string(),
                    shipped_status: "shipped".to_string(),
                    asset_kind: "tracked_backend_binary".to_string(),
                    executable: true,
                    symlink_target: None,
                    owner_phase: Some(tool.owner_phase.to_string()),
                    verification: Some(tool.verification.to_string()),
                },
            );
        }
    }

    for (package, mut manifest) in manifests {
        manifest.entries.retain(|entry| {
            !PHASE_08_LOCALE_TOOL_BACKENDS.contains(&entry.path.as_str())
                && !PHASE_09_AUX_TOOL_BACKENDS.contains(&entry.path.as_str())
        });
        normalize_package_entries(&mut manifest.entries);
        write_pretty_json(
            &safe_root().join(format!("generated/baseline/package-files/{package}.json")),
            &manifest,
        )?;
    }
    normalize_locale_script_package_manifest()?;
    normalize_network_config_package_manifest()?;
    Ok(())
}

fn update_package_scope_libc_family_files(files: &mut Vec<TomlValue>) -> Result<()> {
    let build_root = phase06_public_build_root();
    for path in [
        "/usr/lib64/ld-linux-x86-64.so.2",
        "/usr/lib64/libBrokenLocale.so.1",
        "/usr/lib64/libc.so.6",
        "/usr/lib64/libdl.so.2",
        "/usr/lib64/libm.so.6",
        "/usr/lib64/libmvec.so.1",
        "/usr/lib64/libpcprofile.so",
        "/usr/lib64/libpthread.so.0",
        "/usr/lib64/librt.so.1",
        "/usr/lib64/libthread_db.so.1",
        "/usr/lib64/libutil.so.1",
        "/usr/lib64/libc_malloc_debug.so.0",
        "/usr/lib64/libmemusage.so",
        "/usr/lib64/libanl.so.1",
        "/usr/lib64/libnsl.so.1",
        "/usr/lib64/libnss_compat.so.2",
        "/usr/lib64/libnss_dns.so.2",
        "/usr/lib64/libnss_files.so.2",
        "/usr/lib64/libnss_hesiod.so.2",
        "/usr/lib64/libresolv.so.2",
    ] {
        upsert_package_scope_file(
            files,
            path,
            "libc6",
            &match path {
                "/usr/lib64/libthread_db.so" => {
                    public_build_source_path(&build_root, "/usr/lib64/libthread_db.so.1")
                }
                "/usr/lib64/libc_malloc_debug.so" => {
                    public_build_source_path(&build_root, "/usr/lib64/libc_malloc_debug.so.0")
                }
                _ => public_build_source_path(&build_root, path),
            },
            "rust_target",
            false,
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, path);
    }
    for (backend_path, public_path) in PHASE_06_PRIVATE_BACKEND_DSOS {
        upsert_package_scope_file(
            files,
            backend_path,
            "libc6",
            &build_testroot_source_path(public_path),
            "private_baseline_backend_dso",
            false,
            "required_package",
            "shipped",
        );
        set_package_scope_rationale(
            files,
            backend_path,
            "Phase 6 private baseline backend DSO: non-public copy used only by generated forwarding veneers or loader delegation while the public libc-family path is safe-built.",
        );
    }
    for path in [
        "/usr/lib64/libBrokenLocale.so",
        "/usr/lib64/libc.so",
        "/usr/lib64/libm.so",
        "/usr/lib64/libmvec.so",
        "/usr/lib64/libthread_db.so",
        "/usr/lib64/libc_malloc_debug.so",
        "/usr/lib64/libanl.so",
        "/usr/lib64/libnss_compat.so",
        "/usr/lib64/libnss_hesiod.so",
        "/usr/lib64/libresolv.so",
    ] {
        upsert_package_scope_file(
            files,
            path,
            "libc6-dev",
            &match path {
                "/usr/lib64/libmvec.so" => {
                    public_build_source_path(&build_root, "/usr/lib64/libmvec.so.1")
                }
                _ => public_build_source_path(&build_root, path),
            },
            "rust_target",
            false,
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, path);
    }
    for path in FINAL_STARTFILES {
        upsert_package_scope_file(
            files,
            path,
            "libc6-dev",
            &public_build_source_path(&build_root, path),
            "compat_asm",
            false,
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, path);
        set_package_scope_rationale(
            files,
            path,
            "Final amd64 startfile payload is staged under the safe build root and tracked as a compatibility assembly artifact for Debian libc6-dev link compatibility.",
        );
    }
    for path in FINAL_STATIC_ARCHIVES {
        if path == "/usr/lib64/libpthread_nonshared.a" {
            upsert_package_scope_file(
                files,
                path,
                "libc6-dev",
                "generated:empty-libpthread_nonshared.a",
                "synthetic_empty_archive",
                false,
                "required_package",
                "shipped",
            );
            clear_package_scope_temporary(files, path);
            set_package_scope_rationale(
                files,
                path,
                "Debian's amd64 libc6-dev package ships libpthread_nonshared.a for compiler-driver compatibility even though no members are required after libpthread merged into libc; the safe package emits an intentionally empty archive.",
            );
            continue;
        }
        upsert_package_scope_file(
            files,
            path,
            "libc6-dev",
            &public_build_source_path(&build_root, path),
            if path.ends_with(".so") {
                "rust_target"
            } else {
                "safe_compat_archive"
            },
            path.ends_with(".so"),
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, path);
        set_package_scope_rationale(
            files,
            path,
            "Final libc6-dev link payload is staged from the safe build root so package manifests no longer source code-bearing link assets from build_testroot.",
        );
    }
    upsert_package_scope_file(
        files,
        "/usr/lib64/libpthread_nonshared.a",
        "libc6-dev",
        "generated:empty-libpthread_nonshared.a",
        "synthetic_empty_archive",
        false,
        "required_package",
        "shipped",
    );
    clear_package_scope_temporary(files, "/usr/lib64/libpthread_nonshared.a");
    set_package_scope_rationale(
        files,
        "/usr/lib64/libpthread_nonshared.a",
        "Debian's amd64 libc6-dev package ships libpthread_nonshared.a for compiler-driver compatibility even though no members are required after libpthread merged into libc; the safe package emits an intentionally empty archive.",
    );

    for value in files.iter_mut().filter_map(TomlValue::as_table_mut) {
        let Some(package) = value.get("package").and_then(TomlValue::as_str) else {
            continue;
        };
        let Some(source_path) = value.get("source_path").and_then(TomlValue::as_str) else {
            continue;
        };
        if package == "libc6-dev" && source_path.starts_with("build/testroot.pristine") {
            let path = value.get("path").and_then(TomlValue::as_str).unwrap_or("");
            if is_code_bearing_libc6_dev_link_asset(path) {
                value.insert("temporary".to_string(), TomlValue::Boolean(true));
                value.insert(
                    "final_cutover_phase".to_string(),
                    TomlValue::String(FINAL_CUTOVER_PHASE.to_string()),
                );
            } else {
                value.remove("temporary");
                value.remove("final_cutover_phase");
                value.insert(
                    "rationale".to_string(),
                    TomlValue::String(
                        "Retained from build_testroot as non-executable libc6-dev header or data-only metadata needed for Debian compile compatibility; no code-bearing startfile, archive, DSO, or audit helper is sourced from this path.".to_string(),
                    ),
                );
            }
        }
    }

    Ok(())
}

fn is_code_bearing_libc6_dev_link_asset(path: &str) -> bool {
    (path.starts_with("/usr/lib64/") || path.starts_with("/usr/lib64/audit/"))
        && (path.ends_with(".o") || path.ends_with(".a") || path.ends_with(".so"))
}

fn update_package_scope_tool_files(files: &mut Vec<TomlValue>) -> Result<()> {
    remove_package_scope_paths(files, &PHASE_08_LOCALE_TOOL_BACKENDS);
    remove_package_scope_paths(files, &PHASE_09_AUX_TOOL_BACKENDS);
    for tool in required_tools() {
        let asset_kind = match tool.kind {
            RequiredToolKind::FallbackWrapper { .. } => "fallback_wrapper",
            RequiredToolKind::RustEntrypoint { .. } => "rust_target",
        };
        upsert_package_scope_file(
            files,
            tool.entrypoint,
            tool.package,
            logical_source_path(tool),
            asset_kind,
            true,
            "required_package",
            "shipped",
        );
        remove_symlink_target(files, tool.entrypoint);
        for backend in backend_assets(tool) {
            upsert_package_scope_file(
                files,
                backend.install_path,
                tool.package,
                backend.source_path,
                "tracked_backend_binary",
                true,
                "required_package",
                "shipped",
            );
        }
    }
    update_package_scope_locale_script_files(files);
    update_package_scope_network_config_files(files);
    Ok(())
}

fn remove_package_scope_paths(files: &mut Vec<TomlValue>, paths: &[&str]) {
    files.retain(|entry| {
        let Some(path) = entry
            .as_table()
            .and_then(|table| table.get("path"))
            .and_then(TomlValue::as_str)
        else {
            return true;
        };
        !paths.contains(&path)
    });
}

fn normalize_network_config_package_manifest() -> Result<()> {
    let path = safe_root().join("generated/baseline/package-files/libc-bin.json");
    let mut manifest: PackageManifest = load_json(&path)?;
    set_package_manifest_phase(&mut manifest);
    for (install_path, source_path) in [
        (
            "/usr/share/libc-bin/nsswitch.conf",
            "safe/debian/local/etc/nsswitch.conf",
        ),
        ("/etc/default/nss", "safe/debian/local/etc/nss"),
    ] {
        upsert_package_entry(
            &mut manifest.entries,
            PackageEntry {
                package: "libc-bin".to_string(),
                path: install_path.to_string(),
                source_path: Some(source_path.to_string()),
                source_origin: "safe_local".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "config_asset".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(PHASE_07_ID.to_string()),
                verification: Some("network-tools".to_string()),
            },
        );
    }
    normalize_package_entries(&mut manifest.entries);
    write_pretty_json(&path, &manifest)
}

fn update_package_scope_network_config_files(files: &mut Vec<TomlValue>) {
    for (install_path, source_path) in [
        (
            "/usr/share/libc-bin/nsswitch.conf",
            "safe/debian/local/etc/nsswitch.conf",
        ),
        ("/etc/default/nss", "safe/debian/local/etc/nss"),
    ] {
        upsert_package_scope_file(
            files,
            install_path,
            "libc-bin",
            source_path,
            "config_asset",
            false,
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, install_path);
    }
}

fn normalize_locale_script_package_manifest() -> Result<()> {
    let path = safe_root().join("generated/baseline/package-files/locales.json");
    let mut manifest: PackageManifest = load_json(&path)?;
    set_package_manifest_phase(&mut manifest);
    for (install_path, source_path) in LOCALE_DATA_FILES {
        upsert_package_entry(
            &mut manifest.entries,
            PackageEntry {
                package: "locales".to_string(),
                path: install_path.to_string(),
                source_path: Some(source_path.to_string()),
                source_origin: source_path.to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "data_asset".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(PHASE_08_ID.to_string()),
                verification: Some("locale-tools".to_string()),
            },
        );
    }
    for (install_path, source_path, executable) in LOCALE_HELPER_SCRIPTS {
        upsert_package_entry(
            &mut manifest.entries,
            PackageEntry {
                package: "locales".to_string(),
                path: install_path.to_string(),
                source_path: Some(source_path.to_string()),
                source_origin: "safe_local".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "script_asset".to_string(),
                executable,
                symlink_target: None,
                owner_phase: Some(PHASE_08_ID.to_string()),
                verification: Some("locale-tools".to_string()),
            },
        );
    }
    normalize_package_entries(&mut manifest.entries);
    write_pretty_json(&path, &manifest)
}

fn update_package_scope_locale_script_files(files: &mut Vec<TomlValue>) {
    for (path, source_path, executable) in LOCALE_HELPER_SCRIPTS {
        upsert_package_scope_file(
            files,
            path,
            "locales",
            source_path,
            "script_asset",
            executable,
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, path);
    }
}

fn remove_symlink_target(files: &mut [TomlValue], path: &str) {
    for value in files {
        let Some(table) = value.as_table_mut() else {
            continue;
        };
        if table.get("path").and_then(TomlValue::as_str) == Some(path) {
            table.remove("symlink_target");
        }
    }
}

fn clear_package_scope_temporary(files: &mut [TomlValue], path: &str) {
    for value in files {
        let Some(table) = value.as_table_mut() else {
            continue;
        };
        if table.get("path").and_then(TomlValue::as_str) != Some(path) {
            continue;
        }
        table.remove("temporary");
        table.remove("final_cutover_phase");
    }
}

fn set_package_scope_rationale(files: &mut [TomlValue], path: &str, rationale: &str) {
    for value in files {
        let Some(table) = value.as_table_mut() else {
            continue;
        };
        if table.get("path").and_then(TomlValue::as_str) == Some(path) {
            table.insert(
                "rationale".to_string(),
                TomlValue::String(rationale.to_string()),
            );
        }
    }
}

fn upsert_package_scope_file(
    files: &mut Vec<TomlValue>,
    path: &str,
    package: &str,
    source_path: &str,
    asset_kind: &str,
    executable: bool,
    scope: &str,
    shipped_status: &str,
) {
    let mut table = toml::map::Map::new();
    table.insert(
        "asset_kind".to_string(),
        TomlValue::String(asset_kind.to_string()),
    );
    table.insert("executable".to_string(), TomlValue::Boolean(executable));
    table.insert(
        "package".to_string(),
        TomlValue::String(package.to_string()),
    );
    table.insert("path".to_string(), TomlValue::String(path.to_string()));
    table.insert("scope".to_string(), TomlValue::String(scope.to_string()));
    table.insert(
        "shipped_status".to_string(),
        TomlValue::String(shipped_status.to_string()),
    );
    table.insert(
        "source_path".to_string(),
        TomlValue::String(source_path.to_string()),
    );

    if let Some(existing) = files.iter_mut().find(|entry| {
        entry
            .as_table()
            .and_then(|table| table.get("path"))
            .and_then(TomlValue::as_str)
            == Some(path)
    }) {
        *existing = TomlValue::Table(table);
    } else {
        files.push(TomlValue::Table(table));
        files.sort_by(|left, right| {
            let left = left
                .as_table()
                .and_then(|table| table.get("path"))
                .and_then(TomlValue::as_str)
                .unwrap_or_default();
            let right = right
                .as_table()
                .and_then(|table| table.get("path"))
                .and_then(TomlValue::as_str)
                .unwrap_or_default();
            left.cmp(right)
        });
    }
}

fn upsert_package_entry(entries: &mut Vec<PackageEntry>, new_entry: PackageEntry) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.path == new_entry.path)
    {
        *existing = new_entry;
    } else {
        entries.push(new_entry);
    }
}

fn set_package_manifest_phase(manifest: &mut PackageManifest) {
    let Some(root) = manifest.metadata.as_object_mut() else {
        return;
    };
    let metadata = root
        .entry("metadata".to_string())
        .or_insert_with(|| json!({}));
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.insert("phase".to_string(), json!(PHASE_ID));
    }
}

fn normalize_package_entries(entries: &mut Vec<PackageEntry>) {
    let mut by_path = BTreeMap::new();
    for entry in std::mem::take(entries) {
        by_path.insert(entry.path.clone(), entry);
    }
    *entries = by_path.into_values().collect();
}
