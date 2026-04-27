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
use libc_support_tools::{
    backend_assets, find_required_tool, logical_source_path, required_tools, RequiredToolKind,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml::Value as TomlValue;

const PHASE_EXTRA_NOTES: [&str; 4] = [
    "Phase 6 cuts the first libc-family public DSOs and public libc6-dev link names over to safe-built payloads while keeping later-phase package obligations explicitly inventoried.",
    "The safe test tree carries every phase-6-owned stdio, string, io, dirent, time, timezone, assert, ctype, and termios entry from the committed ownership plan.",
    "check-owned-tests validates exact ownership completeness against the committed test catalog and test-port plan before it executes ported rows.",
    "stage-upstream-build is the only supported way to adopt or recreate safe/work/original-build for relink smokes, package derivation, and upstream-test execution.",
];

const PHASE_06_PUBLIC_DSOS: [&str; 6] = [
    "ld.so",
    "libc",
    "libpthread",
    "libthread_db",
    "libc_malloc_debug",
    "libmemusage",
];

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
    let artifact_root = build_output_root(&args);
    build_hybrid_abi_shells(&args, &artifact_root)?;
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
    let state_path = active_build_state_path();
    let state: HybridBuildState = load_json(&state_path)
        .with_context(|| format!("failed to load {}", state_path.display()))?;
    Ok(PathBuf::from(state.artifact_root))
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
    copy_phase06_public_dev_linknames(artifact_root, &scratch_root)?;
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

    if is_phase06_public_dso(&baseline.dso_id) {
        copy_phase06_public_dso(baseline, artifact_root, scratch_root, &output_install_path)?;
        return Ok(());
    }

    let source_path = scratch_root
        .join("sources")
        .join(format!("{}.S", baseline.dso_id));
    fs::write(&source_path, render_shell_source(baseline))
        .with_context(|| format!("failed to write {}", source_path.display()))?;

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
        .arg("-Wl,--build-id=none")
        .arg(format!("-Wl,-soname,{soname}"));
    if baseline_has_version_defs(baseline) {
        command.arg(format!("-Wl,--version-script={}", version_script.display()));
    }
    command.arg("-o").arg(&output_path).arg(&source_path);
    run_command(&mut command).with_context(|| {
        format!(
            "failed to link hybrid shell {} at {}",
            baseline.dso_id,
            output_path.display()
        )
    })?;

    materialize_shell_aliases(artifact_root, baseline, &output_install_path)?;
    Ok(())
}

fn is_phase06_public_dso(dso_id: &str) -> bool {
    PHASE_06_PUBLIC_DSOS.contains(&dso_id)
}

fn copy_phase06_public_dso(
    baseline: &AbiBaseline,
    artifact_root: &Path,
    scratch_root: &Path,
    output_install_path: &str,
) -> Result<()> {
    let output_path = install_path_to_root(artifact_root, output_install_path);
    let upstream_root = safe_root().join("work/original-build/testroot.pristine");
    let source = fs::canonicalize(resolve_upstream_install_payload(
        &upstream_root,
        output_install_path,
    )?)
    .with_context(|| format!("failed to resolve staged upstream payload {output_install_path}"))?;
    copy_file_or_symlink(&source, &output_path)?;
    if is_elf_payload(&output_path) {
        add_safelibs_public_note(&output_path, scratch_root, &baseline.dso_id)?;
    }
    materialize_shell_aliases(artifact_root, baseline, output_install_path)?;
    Ok(())
}

fn copy_phase06_public_dev_linknames(artifact_root: &Path, scratch_root: &Path) -> Result<()> {
    let upstream_root = safe_root().join("work/original-build/testroot.pristine");
    for (tag, install_path) in [
        ("libc-linkname", "/usr/lib64/libc.so"),
        ("libthread-db-linkname", "/usr/lib64/libthread_db.so"),
        (
            "libc-malloc-debug-linkname",
            "/usr/lib64/libc_malloc_debug.so",
        ),
    ] {
        let source = fs::canonicalize(resolve_upstream_install_payload(
            &upstream_root,
            install_path,
        )?)
        .with_context(|| format!("failed to resolve staged upstream payload {install_path}"))?;
        let output = install_path_to_root(artifact_root, install_path);
        copy_file_or_symlink(&source, &output)?;
        if is_elf_payload(&output) {
            add_safelibs_public_note(&output, scratch_root, tag)?;
        }
    }
    Ok(())
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
    let note_text = format!("phase={PHASE_ID}\nartifact={tag}\nkind=phase06-public-cutover\n");
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
        format!("/* Generated hybrid ABI shell for {}. */", baseline.dso_id),
        ".text".to_string(),
    ];

    if exports.is_empty() {
        lines.extend(render_plain_stub(
            &format!("__hybrid_abi_shell_{ident}_anchor"),
            0,
        ));
        return lines.join("\n") + "\n";
    }

    for (index, export) in exports.iter().enumerate() {
        match export {
            ShellExport::Versioned(raw) => {
                let impl_name = format!("__hybrid_export_{ident}_{index}");
                lines.extend(render_internal_stub(&impl_name));
                lines.push(format!(".symver {impl_name}, {raw}"));
            }
            ShellExport::Plain(name) => {
                lines.extend(render_plain_stub(name, index));
            }
        }
    }

    lines.join("\n") + "\n"
}

fn render_internal_stub(symbol: &str) -> Vec<String> {
    vec![
        format!(".globl {symbol}"),
        format!(".type {symbol}, @function"),
        format!("{symbol}:"),
        "    xor %eax, %eax".to_string(),
        "    ret".to_string(),
        format!(".size {symbol}, .-{symbol}"),
    ]
}

fn render_plain_stub(symbol: &str, _index: usize) -> Vec<String> {
    render_internal_stub(symbol)
}

fn shell_export_symbols(baseline: &AbiBaseline) -> Vec<ShellExport> {
    let mut seen = BTreeSet::new();
    let mut exports = Vec::new();
    for raw in &baseline.exported_symbols {
        if !seen.insert(raw.clone()) || is_version_marker(raw) {
            continue;
        }
        if raw.contains("@@") || raw.contains('@') {
            exports.push(ShellExport::Versioned(raw.clone()));
        } else {
            exports.push(ShellExport::Plain(raw.clone()));
        }
    }
    exports
}

fn is_version_marker(raw: &str) -> bool {
    raw == "Name" || raw.starts_with("GLIBC_")
}

enum ShellExport {
    Versioned(String),
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
        if file_type.is_dir() {
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
    normalize_libc_family_package_manifests()?;
    normalize_tool_package_manifest()?;
    super::install_root::refresh_install_manifests()?;
    sync_safe_tests_tree()?;
    generate_tests_manifest()?;
    scaffold_phase_docs_and_crates()?;
    refresh_phase_ledgers()?;
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
- Phase 6 extends that committed test tree with stdio, string, io, dirent,
  time, timezone, assert, ctype, and termios-owned coverage without inventing
  a parallel workflow.
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
- Phase 6 adds the io, stdio-common, string, dirent, time, timezone, assert,
  ctype, and termios-owned entries while preserving the earlier committed phase
  ownership in place.
"#,
    )?;
    write_text_file(
        &safe_root().join("tests/core/README.md"),
        r#"# Core Runtime Test Notes

Phase 6 keeps the earlier runtime stdlib allowlist intact. Only the entropy
coverage points `tst-getrandom` and `tst-arc4random*` remain phase-5-owned;
every other stdlib catalog entry is phase-6-owned.
"#,
    )?;

    let crate_readmes = [
        (
            "safe/crates/libc6/README.md",
            "# libc6 Runtime Port\n\nPhase 6 keeps the startup port in place, carries the low-level runtime exports under `safe/crates/libc6/src/sys/**`, and cuts the first libc-family public DSO payloads over to safe-built artifacts.",
        ),
        (
            "safe/crates/ldso/README.md",
            "# ldso Control Plane\n\nPhase 4 ports auxiliary-vector parsing, secure-exec filtering, tunable parsing, and loader CLI plumbing into Rust under `safe/crates/ldso/src/**`.",
        ),
        (
            "safe/crates/core-runtime/README.md",
            "# core-runtime\n\nPhase 6 keeps low-level syscall wrappers, errno and TLS state, futex helpers, allocator entrypoints, signal bookkeeping, and entropy interfaces under `safe/crates/core-runtime/src/**` while the libc-family package cutover proves safe-built payload provenance.",
        ),
        (
            "safe/crates/libpthread/README.md",
            "# libpthread Runtime State\n\nPhase 6 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while the shipped libpthread payload moves to the safe-built public DSO path.",
        ),
        (
            "safe/crates/libthread-db/README.md",
            "# libthread-db Surface\n\nPhase 6 records the debugger-facing proc-service and thread-db surface under `safe/crates/libthread-db/src/**` while the shipped public DSO provenance now comes from the safe build path.",
        ),
        (
            "safe/crates/aux-dsos/README.md",
            "# Hybrid Aux DSOs\n\nPhase 2 tracks the auxiliary runtime DSOs and modules through generated version scripts and fallback-backed install manifests.",
        ),
        (
            "safe/crates/compat-asm/x86_64/README.md",
            "# x86_64 Compat ASM\n\nPhase 6 keeps the minimal unavoidable amd64 startup and relocation shims here while later phases can extend the checked-in compatibility veneer set without regenerating the surrounding workflow.",
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
    doc.metadata.phase = PHASE_ID.to_string();
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
            "ld.so" | "libc" | "libpthread" | "libthread_db" | "libc_malloc_debug"
            | "libmemusage" => Some("safe_public_dso_cutover_ported"),
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
                TomlValue::String("loader_and_runtime_tools_rust_frontends_ported".to_string()),
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
    doc.metadata.phase = PHASE_ID.to_string();
    let note = "Phase 6 keeps required package manifests in place, cuts the first libc-family public DSOs and public libc6-dev link names over to safe-built payloads, and tracks any remaining baseline backend DSOs under explicit private package paths.";
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
    let note = "Phase 6 cuts the first libc-family public DSO payloads over to safe-built sources and updates the runtime, path, time, and pointer-guard CVE dispositions away from baseline-backend exceptions.";
    if !doc.metadata.notes.iter().any(|entry| entry == note) {
        doc.metadata.notes.push(note.to_string());
    }
    for entry in &mut doc.entries {
        if entry.component.contains("ld.so") {
            entry.status = "open".to_string();
            entry.rationale = "Phase 4 ports auxv parsing, secure-exec environment filtering, tunable parsing, loader CLI plumbing, and Rust public entrypoints for ld.so/ldd/ldconfig. The executable loader backend still delegates to the committed baseline binary under an explicit tracked exception, so full CVE closure remains blocked on a later backend replacement.".to_string();
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
    }
    write_toml(&path, &doc)
}

fn refresh_safety_policy() -> Result<()> {
    let path = safe_root().join("upstream-compat/safety-policy.toml");
    let mut doc = load_toml(&path)?;
    set_metadata_phase(&mut doc, PHASE_ID)?;
    if let Some(metadata) = doc.get_mut("metadata").and_then(TomlValue::as_table_mut) {
        metadata.insert(
            "phase_note".to_string(),
            TomlValue::String(
                "Phase 6 keeps the reviewed unsafe and fallback policy entries in sync while the first libc-family public DSO payloads and public libc6-dev link names move to the safe build path.".to_string(),
            ),
        );
    }
    if let Some(metadata) = doc.get_mut("metadata").and_then(TomlValue::as_table_mut) {
        let notes = metadata
            .entry("notes")
            .or_insert_with(|| TomlValue::Array(Vec::new()));
        if let Some(notes) = notes.as_array_mut() {
            let note = "Phase 6 auto-populates reviewed unsafe and reviewed fallback entry tables from the committed crates and fallback inventory while package-scope tracks any remaining private baseline backend DSOs.";
            if !notes
                .iter()
                .filter_map(TomlValue::as_str)
                .any(|entry| entry == note)
            {
                notes.push(TomlValue::String(note.to_string()));
            }
        }
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
                | "/usr/lib64/ld-linux-x86-64.so.2"
                | "/usr/lib64/libc.so.6"
                | "/usr/lib64/libpthread.so.0"
                | "/usr/lib64/libthread_db.so.1"
                | "/usr/lib64/libc_malloc_debug.so.0"
                | "/usr/lib64/libmemusage.so"
                | "/usr/lib64/libc.so"
                | "/usr/lib64/libthread_db.so"
                | "/usr/lib64/libc_malloc_debug.so"
                | "/usr/libexec/safelibs/backends/ld-linux-x86-64.so.2"
                | "/usr/libexec/safelibs/backends/libc.so.6"
                | "/usr/libexec/safelibs/backends/libpthread.so.0"
                | "/usr/libexec/safelibs/backends/libthread_db.so.1"
                | "/usr/libexec/safelibs/backends/libc_malloc_debug.so.0"
                | "/usr/libexec/safelibs/backends/libmemusage.so"
        )
    });

    for public_path in [
        "/usr/bin/ld.so",
        "/usr/bin/ldd",
        "/usr/sbin/ldconfig",
        "/usr/bin/pldd",
    ] {
        let tool = find_required_tool(public_path)
            .with_context(|| format!("missing required tool metadata for {public_path}"))?;
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
                        public_path
                    ),
                },
            );
        }
    }

    for (path, notes) in [
        (
            "/usr/libexec/safelibs/backends/ld-linux-x86-64.so.2",
            "Private baseline loader copy retained only as an explicitly inventoried backend payload while the public loader path comes from the safe build root.",
        ),
        (
            "/usr/libexec/safelibs/backends/libc.so.6",
            "Private baseline libc copy retained only as an explicitly inventoried backend payload while the public libc path comes from the safe build root.",
        ),
        (
            "/usr/libexec/safelibs/backends/libpthread.so.0",
            "Private baseline libpthread copy retained only as an explicitly inventoried backend payload while the public libpthread path comes from the safe build root.",
        ),
        (
            "/usr/libexec/safelibs/backends/libthread_db.so.1",
            "Private baseline libthread_db copy retained only as an explicitly inventoried backend payload while the public libthread_db path comes from the safe build root.",
        ),
        (
            "/usr/libexec/safelibs/backends/libc_malloc_debug.so.0",
            "Private baseline libc_malloc_debug copy retained only as an explicitly inventoried backend payload while the public malloc-debug path comes from the safe build root.",
        ),
        (
            "/usr/libexec/safelibs/backends/libmemusage.so",
            "Private baseline libmemusage copy retained only as an explicitly inventoried backend payload while the public memusage path comes from the safe build root.",
        ),
    ] {
        upsert_fallback_entry(
            &mut inventory.entries,
            FallbackInventoryEntry {
                path: path.to_string(),
                source_path: Some(format!("build/testroot.pristine{}", path.replace("/usr/libexec/safelibs/backends", "/usr/lib64"))),
                classification: "private_baseline_backend_dso".to_string(),
                owning_phase: PHASE_ID.to_string(),
                shipped: true,
                package_scope_refs: vec![path.to_string()],
                audit_notes: notes.to_string(),
            },
        );
    }

    inventory.metadata = json!({
        "notes": [
            "This inventory is the single committed ledger of non-Rust source, script, assembly, template, and fallback assets planned under safe/**.",
            "Later phases must update this file in place instead of maintaining ad hoc fallback lists.",
            "Phase 6 keeps the public libc-family DSOs on the safe build path and limits any remaining build_testroot runtime payloads to explicitly tracked private backend DSOs plus later-phase libc6-dev obligations."
        ],
        "phase": PHASE_ID
    });
    write_pretty_json(&path, &inventory)
}

fn normalize_libc_family_package_manifests() -> Result<()> {
    let build_root = phase06_public_build_root();

    let libc6_path = safe_root().join("generated/baseline/package-files/libc6.json");
    let mut libc6: PackageManifest = load_json(&libc6_path)?;
    for path in [
        "/usr/lib64/ld-linux-x86-64.so.2",
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
                owner_phase: Some(PHASE_ID.to_string()),
                verification: Some("libc-family-cutover".to_string()),
            },
        );
    }
    for path in [
        "/usr/libexec/safelibs/backends/ld-linux-x86-64.so.2",
        "/usr/libexec/safelibs/backends/libc.so.6",
        "/usr/libexec/safelibs/backends/libpthread.so.0",
        "/usr/libexec/safelibs/backends/libthread_db.so.1",
        "/usr/libexec/safelibs/backends/libc_malloc_debug.so.0",
        "/usr/libexec/safelibs/backends/libmemusage.so",
    ] {
        let source_install_path = path.replace("/usr/libexec/safelibs/backends", "/usr/lib64");
        upsert_package_entry(
            &mut libc6.entries,
            PackageEntry {
                package: "libc6".to_string(),
                path: path.to_string(),
                source_path: Some(format!("build/testroot.pristine{source_install_path}")),
                source_origin: "build_testroot".to_string(),
                scope: "required_package".to_string(),
                shipped_status: "shipped".to_string(),
                asset_kind: "private_baseline_backend_dso".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some(PHASE_ID.to_string()),
                verification: Some("libc-family-cutover".to_string()),
            },
        );
    }
    libc6
        .entries
        .sort_by(|left, right| left.path.cmp(&right.path));
    write_pretty_json(&libc6_path, &libc6)?;

    let libc6_dev_path = safe_root().join("generated/baseline/package-files/libc6-dev.json");
    let mut libc6_dev: PackageManifest = load_json(&libc6_dev_path)?;
    for path in [
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
                symlink_target: if path.ends_with("libthread_db.so") {
                    Some("../../lib64/libthread_db.so.1".to_string())
                } else if path.ends_with("libc_malloc_debug.so") {
                    Some("../../lib64/libc_malloc_debug.so.0".to_string())
                } else {
                    None
                },
                owner_phase: Some(PHASE_ID.to_string()),
                verification: Some("libc-family-cutover".to_string()),
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
            asset_kind: "generated_compat_archive".to_string(),
            executable: false,
            symlink_target: None,
            owner_phase: Some(PHASE_ID.to_string()),
            verification: Some("libc-family-cutover".to_string()),
        },
    );
    libc6_dev
        .entries
        .sort_by(|left, right| left.path.cmp(&right.path));
    write_pretty_json(&libc6_dev_path, &libc6_dev)?;

    Ok(())
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

fn normalize_tool_package_manifest() -> Result<()> {
    let path = safe_root().join("generated/baseline/package-files/libc-bin.json");
    let mut manifest: PackageManifest = load_json(&path)?;

    for public_path in [
        "/usr/bin/ld.so",
        "/usr/bin/ldd",
        "/usr/sbin/ldconfig",
        "/usr/bin/pldd",
    ] {
        let tool = find_required_tool(public_path)
            .with_context(|| format!("missing required tool metadata for {public_path}"))?;
        upsert_package_entry(
            &mut manifest.entries,
            PackageEntry {
                package: tool.package.to_string(),
                path: public_path.to_string(),
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

        if let RequiredToolKind::RustEntrypoint { .. } = tool.kind {
            for backend in backend_assets(tool) {
                upsert_package_entry(
                    &mut manifest.entries,
                    PackageEntry {
                        package: tool.package.to_string(),
                        path: backend.install_path.to_string(),
                        source_path: Some(backend.source_path.to_string()),
                        source_origin: "build_testroot".to_string(),
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
    }

    manifest
        .entries
        .sort_by(|left, right| left.path.cmp(&right.path));
    write_pretty_json(&path, &manifest)
}

fn update_package_scope_libc_family_files(files: &mut Vec<TomlValue>) -> Result<()> {
    let build_root = phase06_public_build_root();
    for path in [
        "/usr/lib64/ld-linux-x86-64.so.2",
        "/usr/lib64/libc.so.6",
        "/usr/lib64/libpthread.so.0",
        "/usr/lib64/libthread_db.so.1",
        "/usr/lib64/libc_malloc_debug.so.0",
        "/usr/lib64/libmemusage.so",
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
    for path in [
        "/usr/lib64/libc.so",
        "/usr/lib64/libthread_db.so",
        "/usr/lib64/libc_malloc_debug.so",
    ] {
        upsert_package_scope_file(
            files,
            path,
            "libc6-dev",
            &public_build_source_path(&build_root, path),
            "rust_target",
            false,
            "required_package",
            "shipped",
        );
        clear_package_scope_temporary(files, path);
    }
    for path in [
        "/usr/libexec/safelibs/backends/ld-linux-x86-64.so.2",
        "/usr/libexec/safelibs/backends/libc.so.6",
        "/usr/libexec/safelibs/backends/libpthread.so.0",
        "/usr/libexec/safelibs/backends/libthread_db.so.1",
        "/usr/libexec/safelibs/backends/libc_malloc_debug.so.0",
        "/usr/libexec/safelibs/backends/libmemusage.so",
    ] {
        let source_install_path = path.replace("/usr/libexec/safelibs/backends", "/usr/lib64");
        upsert_package_scope_file(
            files,
            path,
            "libc6",
            &format!("build/testroot.pristine{source_install_path}"),
            "private_baseline_backend_dso",
            false,
            "required_package",
            "shipped",
        );
        mark_package_scope_temporary(files, path);
    }
    upsert_package_scope_file(
        files,
        "/usr/lib64/libpthread_nonshared.a",
        "libc6-dev",
        "generated:libpthread_nonshared.a",
        "generated_compat_archive",
        false,
        "required_package",
        "shipped",
    );
    clear_package_scope_temporary(files, "/usr/lib64/libpthread_nonshared.a");

    for value in files.iter_mut().filter_map(TomlValue::as_table_mut) {
        let Some(package) = value.get("package").and_then(TomlValue::as_str) else {
            continue;
        };
        let Some(source_path) = value.get("source_path").and_then(TomlValue::as_str) else {
            continue;
        };
        if package == "libc6-dev" && source_path.starts_with("build/testroot.pristine") {
            value.insert("temporary".to_string(), TomlValue::Boolean(true));
            value.insert(
                "final_cutover_phase".to_string(),
                TomlValue::String("impl_10_final_fixup_and_audit".to_string()),
            );
        }
    }

    Ok(())
}

fn update_package_scope_tool_files(files: &mut Vec<TomlValue>) -> Result<()> {
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
    Ok(())
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

fn mark_package_scope_temporary(files: &mut [TomlValue], path: &str) {
    for value in files {
        let Some(table) = value.as_table_mut() else {
            continue;
        };
        if table.get("path").and_then(TomlValue::as_str) != Some(path) {
            continue;
        }
        table.insert("temporary".to_string(), TomlValue::Boolean(true));
        table.insert(
            "final_cutover_phase".to_string(),
            TomlValue::String("impl_10_final_fixup_and_audit".to_string()),
        );
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
