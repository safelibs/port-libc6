use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use glob::Pattern;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

const TEST_FAMILIES: [&str; 18] = [
    "tests",
    "tests-static",
    "tests-special",
    "tests-container",
    "tests-internal",
    "tests-printers",
    "tests-time64",
    "xtests",
    "xtests-special",
    "xtests-static",
    "xtests-time64",
    "tests-pie",
    "xtests-pie",
    "tests-nolibpthread",
    "tests-mcheck",
    "tests-malloc-check",
    "tests-malloc-hugetlb1",
    "tests-malloc-hugetlb2",
];

const PHASE_IDS: [&str; 10] = [
    "impl_01_safe_bootstrap",
    "impl_02_hybrid_abi_shell",
    "impl_03_packaging_and_harness",
    "impl_04_loader_startup_secure_exec",
    "impl_05_core_runtime_threads_entropy",
    "impl_06_io_stdio_string_path",
    "impl_07_nss_resolver_nscd",
    "impl_08_locale_iconv_posix_parsers",
    "impl_09_math_and_aux_dsos",
    "impl_10_final_fixup_and_audit",
];

const PHASE_02_REPRESENTATIVES: [&str; 6] = [
    "list-tunables",
    "tst-_dl_addr_inside_object",
    "tst-malloc-mcheck",
    "tst-pthread-getattr",
    "tst-setjmp-static",
    "tst-resolv-rotate",
];

const TOP_LEVEL_SPECIALS: [(&str, &str, &[&str]); 6] = [
    (
        "c++-types-check",
        "original/scripts/check-c++-types.sh",
        &["original/c++-types.data"],
    ),
    (
        "check-installed-headers-c",
        "original/scripts/check-installed-headers.sh",
        &[],
    ),
    (
        "check-installed-headers-cxx",
        "original/scripts/check-installed-headers.sh",
        &[],
    ),
    (
        "check-local-headers",
        "original/scripts/check-local-headers.sh",
        &[],
    ),
    (
        "check-wrapper-headers",
        "original/scripts/check-wrapper-headers.py",
        &[],
    ),
    ("lint-makefiles", "original/scripts/lint-makefiles.sh", &[]),
];

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "../original")]
    pub source: PathBuf,
    #[arg(long, default_value = "work/original-build")]
    pub build: PathBuf,
    #[arg(long, default_value = "../dependents.json")]
    pub dependents: PathBuf,
    #[arg(long, default_value = "../relevant_cves.json")]
    pub cves: PathBuf,
    #[arg(long)]
    pub verify: bool,
}

#[derive(Clone, Debug, Serialize)]
struct InstallRootEntry {
    path: String,
    kind: String,
    source_path: String,
    executable: bool,
    size: u64,
    sha256: String,
    symlink_target: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct PackageEntry {
    package: String,
    path: String,
    source_path: Option<String>,
    source_origin: String,
    scope: String,
    shipped_status: String,
    asset_kind: String,
    executable: bool,
    symlink_target: Option<String>,
    owner_phase: Option<String>,
    verification: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct HelperPathRecord {
    path: String,
    source_manifest: String,
    shipped_status: String,
    verification: String,
    owner_phase: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct PackageEntryPoint {
    path: String,
    package: String,
    owner_phase: String,
    verification: String,
    shipped_status: String,
}

#[derive(Clone, Debug, Serialize)]
struct PackageComponent {
    package: String,
    category: String,
    manifest: String,
    entry_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct PackageScopeFile {
    path: String,
    package: String,
    source_path: Option<String>,
    scope: String,
    shipped_status: String,
    asset_kind: String,
    executable: bool,
    symlink_target: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct PackageScopeToml {
    metadata: JsonValue,
    debug_derivation: JsonValue,
    package_components: Vec<PackageComponent>,
    helper_paths: Vec<HelperPathRecord>,
    entrypoints: Vec<PackageEntryPoint>,
    files: Vec<PackageScopeFile>,
}

#[derive(Clone, Debug, Serialize)]
struct CveStatusEntry {
    id: String,
    status: String,
    component: String,
    rationale: String,
}

#[derive(Clone, Debug, Serialize)]
struct CveStatusToml {
    metadata: JsonValue,
    entries: Vec<CveStatusEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct SafetyPolicyToml {
    metadata: JsonValue,
    phases: Vec<String>,
    reviewed_unsafe: JsonValue,
    reviewed_fallback: JsonValue,
    policy: JsonValue,
    phase_modes: BTreeMap<String, JsonValue>,
}

#[derive(Clone, Debug, Serialize)]
struct DsoStatus {
    dso_id: String,
    owner_phase: String,
    abi_baseline: String,
    component: String,
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct SubsystemStatus {
    name: String,
    owner_phase: String,
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct PortStatusToml {
    metadata: JsonValue,
    dso_targets: Vec<DsoStatus>,
    subsystems: Vec<SubsystemStatus>,
    package_components: Vec<JsonValue>,
}

#[derive(Clone, Debug, Serialize)]
struct AbiBaseline {
    dso_id: String,
    primary_oracle: String,
    auxiliary_oracles: Vec<String>,
    installed_paths: Vec<String>,
    build_id: Option<String>,
    soname: Option<String>,
    needed: Vec<String>,
    exported_symbols: Vec<String>,
    map_files: Vec<JsonValue>,
    symlist_files: Vec<JsonValue>,
}

#[derive(Clone, Debug, Serialize)]
struct SumEntry {
    status: String,
    key: String,
    display_name: String,
}

#[derive(Clone, Debug, Serialize)]
struct SumFileRecord {
    path: String,
    counts: BTreeMap<String, usize>,
    entries: Vec<SumEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct TestResults {
    metadata: JsonValue,
    global_counts: BTreeMap<String, usize>,
    sum_files: Vec<SumFileRecord>,
}

#[derive(Clone, Debug, Serialize)]
struct TestCatalogEntry {
    catalog_id: String,
    subdir: String,
    name: String,
    family: String,
    origin_selector: String,
    variant: String,
    has_checked_in_baseline_result: bool,
    requires_container_or_privileged_execution: bool,
    origin_makefiles: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ZeroEntrySubdir {
    subdir: String,
    owner_phase: String,
    status: String,
    destination_root: String,
}

#[derive(Clone, Debug, Serialize)]
struct TestCatalog {
    metadata: JsonValue,
    entries: Vec<TestCatalogEntry>,
    no_executable_subdirs: Vec<ZeroEntrySubdir>,
}

#[derive(Clone, Debug, Serialize)]
struct PortPlanEntry {
    catalog_id: String,
    owner_phase: String,
    destination_path: String,
    source_path: Option<String>,
    companion_assets: Vec<String>,
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct SupportSubtree {
    owner_phase: String,
    source_root: String,
    destination_root: String,
    asset_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct TestPortPlan {
    metadata: JsonValue,
    entries: Vec<PortPlanEntry>,
    support_subtree: SupportSubtree,
    zero_entry_subdirs: Vec<ZeroEntrySubdir>,
}

#[derive(Clone, Debug, Serialize)]
struct FallbackInventoryEntry {
    path: String,
    source_path: Option<String>,
    classification: String,
    owning_phase: String,
    shipped: bool,
    package_scope_refs: Vec<String>,
    audit_notes: String,
}

#[derive(Clone, Debug, Serialize)]
struct FallbackInventory {
    metadata: JsonValue,
    entries: Vec<FallbackInventoryEntry>,
}

#[derive(Clone, Debug)]
struct Rule {
    package: String,
    source_pattern: String,
    destination: Option<String>,
    source_origin: String,
}

#[derive(Clone, Debug)]
struct InstallRuleset {
    rules: Vec<Rule>,
    filtered_lines: Vec<FilteredManifestLine>,
}

#[derive(Clone, Debug)]
struct FilteredManifestLine {
    package: String,
    manifest_path: String,
    source_pattern: String,
    destination: Option<String>,
}

#[derive(Clone, Debug)]
struct PackagingAuthority {
    install_manifests: Vec<(String, String)>,
    install_filters: Vec<InstallManifestFilter>,
}

#[derive(Clone, Debug)]
struct InstallManifestFilter {
    needle: String,
}

#[derive(Clone, Debug)]
struct SourceIndex {
    by_stem: HashMap<String, Vec<String>>,
    all_files: Vec<String>,
}

#[derive(Clone, Debug)]
struct BaselineContext {
    repo_root: PathBuf,
    safe_root: PathBuf,
    source_root: PathBuf,
    build_root: PathBuf,
    dependents_path: PathBuf,
    cves_path: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    let current_dir = std::env::current_dir().context("failed to determine current directory")?;
    let safe_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow!("failed to locate safe workspace root"))?
        .to_path_buf();
    let repo_root = safe_root
        .parent()
        .ok_or_else(|| anyhow!("failed to locate repository root"))?
        .to_path_buf();
    let source_root = resolve_path(&current_dir, &args.source);
    let build_root = resolve_path(&current_dir, &args.build);
    let dependents_path = resolve_path(&current_dir, &args.dependents);
    let cves_path = resolve_path(&current_dir, &args.cves);

    for path in [&source_root, &build_root, &dependents_path, &cves_path] {
        if !path.exists() {
            bail!("required input is missing: {}", path.display());
        }
    }

    let context = BaselineContext {
        repo_root,
        safe_root,
        source_root,
        build_root,
        dependents_path,
        cves_path,
    };

    generate_baseline(&context)?;
    if args.verify {
        verify_outputs(&context)?;
    }
    Ok(())
}

fn generate_baseline(context: &BaselineContext) -> Result<()> {
    let baseline_dir = context.safe_root.join("generated/baseline");
    let abi_dir = baseline_dir.join("abi");
    let package_files_dir = baseline_dir.join("package-files");
    let security_dir = context.safe_root.join("generated/security");
    let upstream_dir = context.safe_root.join("upstream-compat");

    fs::create_dir_all(&abi_dir)?;
    fs::create_dir_all(&package_files_dir)?;
    fs::create_dir_all(&security_dir)?;
    fs::create_dir_all(&upstream_dir)?;

    let dependents: JsonValue = read_json(&context.dependents_path)?;
    validate_dependents(&dependents)?;

    let cves: JsonValue = read_json(&context.cves_path)?;
    let relevant_cves = cves
        .get("relevant_cves")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("relevant_cves.json is missing the relevant_cves array"))?;

    let install_root_entries = collect_install_root_entries(context)?;
    write_json(
        &baseline_dir.join("install-root-files.json"),
        &json!({
            "metadata": {
                "phase": "impl_01_safe_bootstrap",
                "generated_from": [
                    "build/testroot.pristine"
                ]
            },
            "entries": install_root_entries
        }),
    )?;

    let packaging = load_packaging_authority(context)?;
    let ruleset = build_install_rules(context, &packaging)?;
    let package_entries = resolve_package_entries(context, &install_root_entries, &ruleset)?;
    write_package_files(context, &package_entries, &package_files_dir)?;
    let package_scope = build_package_scope(&package_entries);
    write_toml(&upstream_dir.join("package-scope.toml"), &package_scope)?;

    let abi_baselines = collect_abi_baselines(context, &package_entries)?;
    for (filename, baseline) in &abi_baselines {
        write_json(&abi_dir.join(filename), baseline)?;
    }

    let test_results = collect_test_results(context)?;
    write_json(&baseline_dir.join("test-results.json"), &test_results)?;

    let source_index = build_source_index(context)?;
    let queried = query_test_families(context)?;
    let (catalog, port_plan) =
        build_test_catalog_and_plan(context, &queried, &test_results, &source_index)?;
    write_json(&baseline_dir.join("test-catalog.json"), &catalog)?;
    write_json(&baseline_dir.join("test-port-plan.json"), &port_plan)?;

    let fallback_inventory = build_fallback_inventory(context, &port_plan, &package_entries)?;
    write_json(
        &baseline_dir.join("fallback-c-inventory.json"),
        &fallback_inventory,
    )?;

    let relevant_index = build_relevant_cve_index(&cves)?;
    write_json(
        &security_dir.join("relevant-cves-index.json"),
        &relevant_index,
    )?;

    let cve_status = build_cve_status(relevant_cves)?;
    write_toml(&upstream_dir.join("cve-status.toml"), &cve_status)?;

    let safety_policy = build_safety_policy();
    write_toml(&upstream_dir.join("safety-policy.toml"), &safety_policy)?;

    let port_status = build_port_status(&abi_baselines);
    write_toml(&upstream_dir.join("port-status.toml"), &port_status)?;

    let readme = build_readme(
        &package_entries,
        &catalog,
        relevant_cves.len(),
        dependents
            .get("dependents")
            .and_then(JsonValue::as_array)
            .map(Vec::len)
            .unwrap_or(0),
    );
    fs::write(context.safe_root.join("README.md"), readme)?;

    Ok(())
}

fn verify_outputs(context: &BaselineContext) -> Result<()> {
    let baseline_dir = context.safe_root.join("generated/baseline");
    let abi_dir = baseline_dir.join("abi");
    let package_files_dir = baseline_dir.join("package-files");
    let upstream_dir = context.safe_root.join("upstream-compat");
    let security_dir = context.safe_root.join("generated/security");

    for path in [
        baseline_dir.join("install-root-files.json"),
        baseline_dir.join("test-catalog.json"),
        baseline_dir.join("test-port-plan.json"),
        baseline_dir.join("test-results.json"),
        baseline_dir.join("fallback-c-inventory.json"),
        package_files_dir.join("libc6-dbg.json"),
        security_dir.join("relevant-cves-index.json"),
        context.safe_root.join("xtask/src/commands/audit_safety.rs"),
        upstream_dir.join("port-status.toml"),
        upstream_dir.join("cve-status.toml"),
        upstream_dir.join("package-scope.toml"),
        upstream_dir.join("safety-policy.toml"),
        context.safe_root.join("README.md"),
    ] {
        if !path.exists() {
            bail!("expected generated output is missing: {}", path.display());
        }
    }

    let abi_count = fs::read_dir(&abi_dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension() == Some(OsStr::new("json")))
        .count();
    if abi_count != 20 {
        bail!("expected 20 ABI baseline files, found {abi_count}");
    }

    let catalog: JsonValue = read_json(&baseline_dir.join("test-catalog.json"))?;
    let entries = catalog
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("test-catalog.json is missing entries"))?;
    let names: HashSet<String> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(JsonValue::as_str))
        .map(str::to_string)
        .collect();
    for name in [
        "c++-types-check",
        "check-installed-headers-c",
        "check-installed-headers-cxx",
        "check-local-headers",
        "check-wrapper-headers",
        "lint-makefiles",
        "list-tunables",
        "tst-_dl_addr_inside_object",
        "tst-malloc-mcheck",
        "tst-pthread-getattr",
        "tst-setjmp-static",
        "tst-resolv-rotate",
    ] {
        if !names.contains(name) {
            bail!("test-catalog.json is missing required entry {name}");
        }
    }

    let no_exec = catalog
        .get("no_executable_subdirs")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("test-catalog.json is missing no_executable_subdirs"))?;
    let zero_subdirs: HashSet<String> = no_exec
        .iter()
        .filter_map(|entry| entry.get("subdir").and_then(JsonValue::as_str))
        .map(str::to_string)
        .collect();
    for subdir in ["manual", "po", "nscd"] {
        if !zero_subdirs.contains(subdir) {
            bail!("test-catalog.json must record zero-entry baseline subdir {subdir}");
        }
    }

    let package_scope: toml::Value = toml::from_str(&fs::read_to_string(
        upstream_dir.join("package-scope.toml"),
    )?)?;
    let helper_paths = package_scope
        .get("helper_paths")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow!("package-scope.toml is missing helper_paths"))?;
    let mut saw_pt_chown = false;
    let mut saw_pldd = false;
    for helper in helper_paths {
        let helper = helper
            .as_table()
            .ok_or_else(|| anyhow!("helper_paths entries must be tables"))?;
        let path = helper
            .get("path")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| anyhow!("helper path is missing path"))?;
        let shipped_status = helper
            .get("shipped_status")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| anyhow!("helper path is missing shipped_status"))?;
        if path == "/usr/lib/pt_chown" {
            saw_pt_chown = shipped_status == "omitted_on_amd64";
        }
        if path == "/usr/bin/pldd" {
            saw_pldd = shipped_status == "shipped";
        }
    }
    if !saw_pt_chown || !saw_pldd {
        bail!("package-scope.toml must record pt_chown omission and pldd shipping");
    }

    println!("ingest-baseline: verification passed");
    Ok(())
}

fn validate_dependents(dependents: &JsonValue) -> Result<()> {
    let entries = dependents
        .get("dependents")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("dependents.json is missing the dependents array"))?;
    if entries.is_empty() {
        bail!("dependents.json must contain at least one dependent entry");
    }
    Ok(())
}

fn collect_install_root_entries(context: &BaselineContext) -> Result<Vec<InstallRootEntry>> {
    let install_root = context.build_root.join("testroot.pristine");
    if !install_root.exists() {
        bail!("missing install root at {}", install_root.display());
    }
    let mut entries = Vec::new();
    for entry in WalkDir::new(&install_root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_dir() {
            continue;
        }
        let rel = path.strip_prefix(&install_root).with_context(|| {
            format!(
                "failed to strip install-root prefix from {}",
                path.display()
            )
        })?;
        let rel_string = rel.to_string_lossy().replace('\\', "/");
        let source_path = format!("build/testroot.pristine/{rel_string}");
        let logical_path = format!("/{}", rel_string);
        let metadata = fs::symlink_metadata(path)?;
        let kind = if metadata.file_type().is_symlink() {
            "symlink"
        } else {
            "file"
        };
        let sha256 = if metadata.file_type().is_file() {
            sha256_path(path)?
        } else {
            String::new()
        };
        let symlink_target = if metadata.file_type().is_symlink() {
            Some(fs::read_link(path)?.to_string_lossy().replace('\\', "/"))
        } else {
            None
        };
        entries.push(InstallRootEntry {
            path: logical_path,
            kind: kind.to_string(),
            source_path,
            executable: metadata.permissions().mode() & 0o111 != 0,
            size: metadata.len(),
            sha256,
            symlink_target,
        });
    }
    Ok(entries)
}

fn load_packaging_authority(context: &BaselineContext) -> Result<PackagingAuthority> {
    let control_path = context.source_root.join("debian/control");
    let rules_path = context.source_root.join("debian/rules");
    let sysdeps_path = context.source_root.join("debian/sysdeps/linux.mk");
    let debhelper_path = context.source_root.join("debian/rules.d/debhelper.mk");

    let control_text = fs::read_to_string(&control_path)
        .with_context(|| format!("failed to read {}", control_path.display()))?;
    let rules_text = fs::read_to_string(&rules_path)
        .with_context(|| format!("failed to read {}", rules_path.display()))?;
    let sysdeps_text = fs::read_to_string(&sysdeps_path)
        .with_context(|| format!("failed to read {}", sysdeps_path.display()))?;
    let debhelper_text = fs::read_to_string(&debhelper_path)
        .with_context(|| format!("failed to read {}", debhelper_path.display()))?;

    if !rules_text.contains("-include debian/sysdeps/$(DEB_HOST_ARCH_OS).mk") {
        bail!(
            "debian/rules no longer includes the host-os sysdeps authority needed for package normalization"
        );
    }
    if !rules_text.contains("include debian/rules.d/*.mk") {
        bail!("debian/rules no longer includes debian/rules.d/*.mk");
    }

    let declared_packages = parse_control_packages(&control_text)?;
    let assignments = parse_make_assignments(&sysdeps_text);
    let libc_package = assignments
        .get("libc")
        .cloned()
        .ok_or_else(|| anyhow!("missing libc assignment in {}", sysdeps_path.display()))?;
    let install_filters = parse_install_manifest_filters(&debhelper_text, &assignments)?;
    let install_manifests = collect_install_manifests(context, &declared_packages, &libc_package)?;

    Ok(PackagingAuthority {
        install_manifests,
        install_filters,
    })
}

fn parse_control_packages(contents: &str) -> Result<BTreeSet<String>> {
    let package_re = Regex::new(r"^Package:\s*(\S+)\s*$").expect("valid package regex");
    let mut packages = BTreeSet::new();
    for line in contents.lines() {
        if let Some(captures) = package_re.captures(line.trim()) {
            packages.insert(captures[1].to_string());
        }
    }
    if packages.is_empty() {
        bail!("debian/control did not declare any binary packages");
    }
    Ok(packages)
}

fn parse_make_assignments(contents: &str) -> BTreeMap<String, String> {
    let assign_re =
        Regex::new(r"^([A-Za-z0-9_]+)\s*(?::=|\?=|=)\s*(.*?)\s*$").expect("valid make regex");
    let mut assignments = BTreeMap::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(captures) = assign_re.captures(trimmed) {
            assignments.insert(captures[1].to_string(), captures[2].trim().to_string());
        }
    }
    assignments
}

fn parse_install_manifest_filters(
    contents: &str,
    assignments: &BTreeMap<String, String>,
) -> Result<Vec<InstallManifestFilter>> {
    let mut filters = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("sed -e \"/") || !trimmed.contains("/d\"") {
            continue;
        }
        let Some((_, after_filter)) = trimmed.split_once("$(filter $(") else {
            continue;
        };
        let Some((variable, remainder)) = after_filter.split_once("),") else {
            continue;
        };
        let Some((expected_value, remainder)) = remainder.split_once("),sed -e \"/") else {
            continue;
        };
        let Some((needle, _)) = remainder.split_once("/d\"") else {
            continue;
        };
        let variable = variable.trim().to_string();
        let expected_value = expected_value.trim().to_string();
        if assignments
            .get(&variable)
            .map(|value| value == &expected_value)
            .unwrap_or(false)
        {
            filters.push(InstallManifestFilter {
                needle: needle.to_string(),
            });
        }
    }
    if filters.is_empty() {
        bail!("failed to derive any debhelper install filters from debian/rules.d/debhelper.mk");
    }
    Ok(filters)
}

fn collect_install_manifests(
    context: &BaselineContext,
    declared_packages: &BTreeSet<String>,
    libc_package: &str,
) -> Result<Vec<(String, String)>> {
    let mut manifests = Vec::new();
    for entry in WalkDir::new(context.source_root.join("debian/debhelper.in"))
        .max_depth(1)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(filename) = entry.file_name().to_str() else {
            continue;
        };
        let Some(stem) = filename.strip_suffix(".install") else {
            continue;
        };
        let Some(package) = manifest_package_for_stem(stem, libc_package, declared_packages) else {
            continue;
        };
        manifests.push((package, repo_rel(context, entry.path())?));
    }
    if manifests.is_empty() {
        bail!("failed to derive any authoritative .install manifests from debian/control");
    }
    manifests.sort_by_key(|(package, _)| install_manifest_order(package));
    Ok(manifests)
}

fn manifest_package_for_stem(
    stem: &str,
    libc_package: &str,
    declared_packages: &BTreeSet<String>,
) -> Option<String> {
    let package = match stem {
        "libc" => libc_package.to_string(),
        "libc-dev" => format!("{libc_package}-dev"),
        "libc-bin" | "libc-dev-bin" | "libc-devtools" | "libc-l10n" | "locales" | "locales-all"
        | "nscd" => stem.to_string(),
        _ => return None,
    };
    declared_packages.contains(&package).then_some(package)
}

fn install_manifest_order(package: &str) -> usize {
    match package {
        "libc6" => 0,
        "libc-bin" => 1,
        "libc6-dev" => 2,
        "libc-dev-bin" => 3,
        "locales" => 4,
        "nscd" => 5,
        "libc-devtools" => 6,
        "libc-l10n" => 7,
        "locales-all" => 8,
        _ => usize::MAX,
    }
}

fn build_install_rules(
    context: &BaselineContext,
    packaging: &PackagingAuthority,
) -> Result<InstallRuleset> {
    let mut rules = Vec::new();
    let mut filtered_lines = Vec::new();
    for (package, manifest_rel) in &packaging.install_manifests {
        let manifest_path = context.repo_root.join(manifest_rel);
        let contents = fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        for raw_line in contents.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let source_pattern = parts
                .next()
                .ok_or_else(|| anyhow!("malformed install manifest line in {manifest_rel}"))?;
            let destination = parts.next().map(|value| value.to_string());
            if packaging
                .install_filters
                .iter()
                .any(|filter| source_pattern.contains(&filter.needle))
            {
                filtered_lines.push(FilteredManifestLine {
                    package: package.clone(),
                    manifest_path: manifest_rel.clone(),
                    source_pattern: source_pattern.to_string(),
                    destination,
                });
                continue;
            }
            rules.push(Rule {
                package: package.clone(),
                source_pattern: substitute_manifest_pattern(source_pattern),
                destination: destination.as_deref().map(substitute_manifest_pattern),
                source_origin: if source_pattern.starts_with("debian/local/") {
                    "debian_local".to_string()
                } else if source_pattern.starts_with("debian/script.in/") {
                    "debian_script".to_string()
                } else if source_pattern.starts_with("nscd/") {
                    "source_tree".to_string()
                } else {
                    "build_testroot".to_string()
                },
            });
        }
    }

    Ok(InstallRuleset {
        rules,
        filtered_lines,
    })
}

fn resolve_package_entries(
    context: &BaselineContext,
    install_root_entries: &[InstallRootEntry],
    ruleset: &InstallRuleset,
) -> Result<Vec<PackageEntry>> {
    let install_map: BTreeMap<String, InstallRootEntry> = install_root_entries
        .iter()
        .cloned()
        .map(|entry| (entry.source_path.clone(), entry))
        .collect();
    let local_files = collect_prefixed_files(
        &context.source_root.join("debian/local"),
        "original/debian/local",
    )?;
    let script_files = collect_prefixed_files(
        &context.source_root.join("debian/script.in"),
        "original/debian/script.in",
    )?;
    let nscd_files = collect_prefixed_files(&context.source_root.join("nscd"), "original/nscd")?;

    let mut by_path: BTreeMap<String, PackageEntry> = BTreeMap::new();
    let install_root = context.build_root.join("testroot.pristine");

    for rule in &ruleset.rules {
        match rule.source_origin.as_str() {
            "build_testroot" => {
                let patterns = expand_build_patterns(&rule.source_pattern);
                for entry in install_root_entries {
                    let source_rel = entry
                        .source_path
                        .trim_start_matches("build/testroot.pristine/")
                        .to_string();
                    if !matches_any_pattern(&source_rel, &patterns) {
                        continue;
                    }
                    let package_path = apply_destination(
                        &rule.source_pattern,
                        rule.destination.as_deref(),
                        &source_rel,
                    );
                    upsert_package_entry(
                        &mut by_path,
                        &rule.package,
                        &package_path,
                        Some(entry.source_path.clone()),
                        Some(context.repo_root.join(&entry.source_path)),
                        &rule.source_origin,
                        rule.package.as_str(),
                        entry.executable,
                        entry.symlink_target.clone(),
                    );
                }
            }
            "debian_local" => {
                for (source_path, relative) in &local_files {
                    if !matches_pattern(relative, &rule.source_pattern) {
                        continue;
                    }
                    let package_path = apply_destination(
                        &rule.source_pattern,
                        rule.destination.as_deref(),
                        relative,
                    );
                    let metadata = fs::symlink_metadata(context.repo_root.join(source_path))?;
                    upsert_package_entry(
                        &mut by_path,
                        &rule.package,
                        &package_path,
                        Some(source_path.clone()),
                        Some(context.repo_root.join(source_path)),
                        &rule.source_origin,
                        rule.package.as_str(),
                        metadata.permissions().mode() & 0o111 != 0,
                        None,
                    );
                }
            }
            "debian_script" => {
                for (source_path, relative) in &script_files {
                    if !matches_pattern(relative, &rule.source_pattern) {
                        continue;
                    }
                    let package_path = apply_destination(
                        &rule.source_pattern,
                        rule.destination.as_deref(),
                        relative,
                    );
                    let metadata = fs::symlink_metadata(context.repo_root.join(source_path))?;
                    upsert_package_entry(
                        &mut by_path,
                        &rule.package,
                        &package_path,
                        Some(source_path.clone()),
                        Some(context.repo_root.join(source_path)),
                        &rule.source_origin,
                        rule.package.as_str(),
                        metadata.permissions().mode() & 0o111 != 0,
                        None,
                    );
                }
            }
            "source_tree" => {
                for (source_path, relative) in &nscd_files {
                    if !matches_pattern(relative, &rule.source_pattern) {
                        continue;
                    }
                    let package_path = apply_destination(
                        &rule.source_pattern,
                        rule.destination.as_deref(),
                        relative,
                    );
                    let metadata = fs::symlink_metadata(context.repo_root.join(source_path))?;
                    upsert_package_entry(
                        &mut by_path,
                        &rule.package,
                        &package_path,
                        Some(source_path.clone()),
                        Some(context.repo_root.join(source_path)),
                        &rule.source_origin,
                        rule.package.as_str(),
                        metadata.permissions().mode() & 0o111 != 0,
                        None,
                    );
                }
            }
            origin => bail!("unsupported rule origin {origin}"),
        }
    }

    let mut matched_install_sources = HashSet::new();
    for entry in by_path.values() {
        if let Some(source_path) = &entry.source_path {
            if source_path.starts_with("build/testroot.pristine/") {
                matched_install_sources.insert(source_path.clone());
            }
        }
    }

    for entry in install_root_entries {
        if matched_install_sources.contains(&entry.source_path) {
            continue;
        }
        let package_path = entry.path.clone();
        by_path
            .entry(package_path.clone())
            .or_insert_with(|| PackageEntry {
                package: "testroot-only".to_string(),
                path: package_path,
                source_path: Some(entry.source_path.clone()),
                source_origin: "build_testroot".to_string(),
                scope: "testroot_only".to_string(),
                shipped_status: "testroot_only".to_string(),
                asset_kind: classify_asset_kind(
                    context.repo_root.join(&entry.source_path),
                    &entry.path,
                    entry.executable,
                ),
                executable: entry.executable,
                symlink_target: entry.symlink_target.clone(),
                owner_phase: None,
                verification: None,
            });
    }

    let libc6_debug_entries = derive_libc6_dbg_entries(context, &by_path)?;
    for debug_entry in libc6_debug_entries {
        by_path.insert(debug_entry.path.clone(), debug_entry);
    }

    let mut entries: Vec<PackageEntry> = by_path.into_values().collect();
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    for helper_absence in omitted_helper_entries(&ruleset.filtered_lines) {
        entries.push(helper_absence);
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    // Sanity-check a few authoritative paths.
    let install_stamp = install_root.join("install.stamp");
    if !install_stamp.exists() {
        bail!("expected install stamp at {}", install_stamp.display());
    }

    let _ = install_map;
    Ok(entries)
}

fn omitted_helper_entries(filtered_lines: &[FilteredManifestLine]) -> Vec<PackageEntry> {
    let mut entries = Vec::new();
    for line in filtered_lines {
        let source_pattern = substitute_manifest_pattern(&line.source_pattern);
        let destination = line.destination.as_deref().map(substitute_manifest_pattern);
        let packaged_path =
            apply_destination(&source_pattern, destination.as_deref(), &source_pattern);
        if line.package == "libc-bin" && packaged_path == "/usr/lib/pt_chown" {
            entries.push(PackageEntry {
                package: "libc-bin".to_string(),
                path: packaged_path,
                source_path: None,
                source_origin: line.manifest_path.clone(),
                scope: "required_package".to_string(),
                shipped_status: "omitted_on_amd64".to_string(),
                asset_kind: "temporary_fallback_binary".to_string(),
                executable: false,
                symlink_target: None,
                owner_phase: Some("impl_05_core_runtime_threads_entropy".to_string()),
                verification: Some("runtime-tools:absence-check".to_string()),
            });
        }
    }
    entries
}

fn write_package_files(
    context: &BaselineContext,
    entries: &[PackageEntry],
    package_files_dir: &Path,
) -> Result<()> {
    let packages = [
        "libc6",
        "libc-bin",
        "libc6-dev",
        "libc-dev-bin",
        "locales",
        "nscd",
        "libc6-dbg",
        "libc-devtools",
        "libc-l10n",
        "locales-all",
        "testroot-only",
    ];

    for package in packages {
        let package_entries: Vec<&PackageEntry> = entries
            .iter()
            .filter(|entry| entry.package == package)
            .collect();
        let scope = match package {
            "libc6" | "libc-bin" | "libc6-dev" | "libc-dev-bin" | "locales" | "nscd"
            | "libc6-dbg" => "required_package",
            "testroot-only" => "testroot_only",
            _ => "deferred_package",
        };
        let filename = format!("{package}.json");
        write_json(
            &package_files_dir.join(filename),
            &json!({
                "metadata": {
                    "package": package,
                    "scope": scope,
                    "phase": "impl_01_safe_bootstrap",
                    "generated_from": [
                        "build/testroot.pristine",
                        "original/debian/control",
                        "original/debian/rules",
                        "original/debian/debhelper.in/*",
                        "original/debian/local/**",
                        "original/debian/script.in/**",
                        "original/debian/rules.d/debhelper.mk",
                        "original/debian/sysdeps/linux.mk"
                    ]
                },
                "entries": package_entries
            }),
        )?;
    }
    let dbg_path = package_files_dir.join("libc6-dbg.json");
    if !dbg_path.exists() {
        bail!("failed to create {}", dbg_path.display());
    }
    let _ = context;
    Ok(())
}

fn build_package_scope(entries: &[PackageEntry]) -> PackageScopeToml {
    let mut package_components = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in entries {
        *counts.entry(entry.package.as_str()).or_insert(0) += 1;
    }
    for (package, count) in counts {
        let category = match package {
            "libc6" | "libc-bin" | "libc6-dev" | "libc-dev-bin" | "locales" | "nscd"
            | "libc6-dbg" => "required_package",
            "testroot-only" => "testroot_only",
            _ => "deferred_package",
        };
        package_components.push(PackageComponent {
            package: package.to_string(),
            category: category.to_string(),
            manifest: format!("safe/generated/baseline/package-files/{package}.json"),
            entry_count: count,
        });
    }

    let helper_specs = [
        (
            "/usr/bin/pldd",
            "original/debian/debhelper.in/libc-bin.install",
            "runtime-tools",
            Some("impl_05_core_runtime_threads_entropy"),
            "shipped",
        ),
        (
            "/usr/lib/pt_chown",
            "original/debian/debhelper.in/libc-bin.install",
            "runtime-tools:absence-check",
            Some("impl_05_core_runtime_threads_entropy"),
            "omitted_on_amd64",
        ),
        (
            "/usr/sbin/ldconfig",
            "original/debian/debhelper.in/libc-bin.install",
            "loader-tools",
            Some("impl_04_loader_startup_secure_exec"),
            "shipped",
        ),
        (
            "/usr/bin/ldd",
            "original/debian/debhelper.in/libc-bin.install",
            "loader-tools",
            Some("impl_04_loader_startup_secure_exec"),
            "shipped",
        ),
        (
            "/usr/bin/ld.so",
            "original/debian/debhelper.in/libc-bin.install",
            "loader-tools",
            Some("impl_04_loader_startup_secure_exec"),
            "shipped",
        ),
        (
            "/usr/bin/getent",
            "original/debian/debhelper.in/libc-bin.install",
            "network-tools",
            Some("impl_07_nss_resolver_nscd"),
            "shipped",
        ),
        (
            "/usr/sbin/nscd",
            "original/debian/debhelper.in/nscd.install",
            "network-tools",
            Some("impl_07_nss_resolver_nscd"),
            "shipped",
        ),
        (
            "/usr/bin/iconv",
            "original/debian/debhelper.in/libc-bin.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/sbin/iconvconfig",
            "original/debian/debhelper.in/libc-bin.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/bin/localedef",
            "original/debian/debhelper.in/libc-bin.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/bin/locale",
            "original/debian/debhelper.in/libc-bin.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/sbin/locale-gen",
            "original/debian/debhelper.in/locales.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/sbin/update-locale",
            "original/debian/debhelper.in/locales.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/sbin/validlocale",
            "original/debian/debhelper.in/locales.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/share/locales/install-language-pack",
            "original/debian/debhelper.in/locales.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/share/locales/remove-language-pack",
            "original/debian/debhelper.in/locales.install",
            "locale-tools",
            Some("impl_08_locale_iconv_posix_parsers"),
            "shipped",
        ),
        (
            "/usr/bin/gencat",
            "original/debian/debhelper.in/libc-dev-bin.install",
            "dev-and-time-tools",
            Some("impl_09_math_and_aux_dsos"),
            "shipped",
        ),
        (
            "/usr/bin/getconf",
            "original/debian/debhelper.in/libc-bin.install",
            "dev-and-time-tools",
            Some("impl_09_math_and_aux_dsos"),
            "shipped",
        ),
        (
            "/usr/bin/tzselect",
            "original/debian/debhelper.in/libc-bin.install",
            "dev-and-time-tools",
            Some("impl_09_math_and_aux_dsos"),
            "shipped",
        ),
        (
            "/usr/bin/zdump",
            "original/debian/debhelper.in/libc-bin.install",
            "dev-and-time-tools",
            Some("impl_09_math_and_aux_dsos"),
            "shipped",
        ),
        (
            "/usr/sbin/zic",
            "original/debian/debhelper.in/libc-bin.install",
            "dev-and-time-tools",
            Some("impl_09_math_and_aux_dsos"),
            "shipped",
        ),
    ];
    let helper_paths = helper_specs
        .into_iter()
        .map(
            |(path, source_manifest, verification, owner_phase, default_status)| {
                let shipped_status = helper_shipped_status(entries, path, default_status);
                helper_record(
                    path,
                    source_manifest,
                    &shipped_status,
                    verification,
                    owner_phase,
                )
            },
        )
        .collect::<Vec<_>>();

    let entrypoints = helper_paths
        .iter()
        .filter(|helper| helper.shipped_status == "shipped" || helper.path == "/usr/lib/pt_chown")
        .map(|helper| PackageEntryPoint {
            path: helper.path.clone(),
            package: package_for_entrypoint(&helper.path).to_string(),
            owner_phase: helper.owner_phase.clone().unwrap_or_default(),
            verification: helper.verification.clone(),
            shipped_status: helper.shipped_status.clone(),
        })
        .collect();

    let files = entries
        .iter()
        .map(|entry| PackageScopeFile {
            path: entry.path.clone(),
            package: entry.package.clone(),
            source_path: entry.source_path.clone(),
            scope: entry.scope.clone(),
            shipped_status: entry.shipped_status.clone(),
            asset_kind: entry.asset_kind.clone(),
            executable: entry.executable,
            symlink_target: entry.symlink_target.clone(),
        })
        .collect();

    PackageScopeToml {
        metadata: json!({
            "phase": "impl_01_safe_bootstrap",
            "platform": "ubuntu-24.04-amd64",
            "notes": [
                "All package baselines are derived from the checked-in build/testroot.pristine tree plus the authoritative Debian packaging manifests in original/debian/**.",
                "Original inputs remain authoritative and are consumed in place; the generated manifests under safe/** are committed views over those inputs.",
                "libc6-dbg is derived only from the libc6 ELF payload baseline and not from libc-bin, libc-dev-bin, or nscd."
            ]
        }),
        debug_derivation: json!({
            "package": "libc6-dbg",
            "derived_from_package": "libc6",
            "excludes_packages": ["libc-bin", "libc-dev-bin", "nscd"]
        }),
        package_components,
        helper_paths,
        entrypoints,
        files,
    }
}

fn build_cve_status(relevant_cves: &[JsonValue]) -> Result<CveStatusToml> {
    let mut entries = Vec::new();
    for cve in relevant_cves {
        let id = json_string(cve.get("id"))?;
        let component = json_string(cve.get("component"))?;
        entries.push(CveStatusEntry {
            id,
            status: "open".to_string(),
            component,
            rationale: "Imported from relevant_cves.json during phase-1 baseline capture; no Rust-side mitigation has been implemented yet.".to_string(),
        });
    }
    Ok(CveStatusToml {
        metadata: json!({
            "phase": "impl_01_safe_bootstrap",
            "default_status": "open"
        }),
        entries,
    })
}

fn build_safety_policy() -> SafetyPolicyToml {
    let mut phase_modes = BTreeMap::new();
    phase_modes.insert(
        "impl_01_safe_bootstrap".to_string(),
        json!({"strongest_mode": "--verify-policy"}),
    );
    phase_modes.insert(
        "impl_02_hybrid_abi_shell".to_string(),
        json!({"strongest_mode": "--verify-policy"}),
    );
    phase_modes.insert(
        "impl_03_packaging_and_harness".to_string(),
        json!({"strongest_mode": "--verify-policy"}),
    );
    phase_modes.insert(
        "impl_04_loader_startup_secure_exec".to_string(),
        json!({"strongest_mode": "--verify-policy"}),
    );
    phase_modes.insert(
        "impl_05_core_runtime_threads_entropy".to_string(),
        json!({"strongest_mode": "--deny-unreviewed-unsafe --deny-untracked-fallback-c"}),
    );
    phase_modes.insert(
        "impl_06_io_stdio_string_path".to_string(),
        json!({"strongest_mode": "--deny-unreviewed-unsafe --deny-untracked-fallback-c"}),
    );
    phase_modes.insert(
        "impl_07_nss_resolver_nscd".to_string(),
        json!({"strongest_mode": "--deny-unreviewed-unsafe --deny-untracked-fallback-c"}),
    );
    phase_modes.insert(
        "impl_08_locale_iconv_posix_parsers".to_string(),
        json!({"strongest_mode": "--deny-unreviewed-unsafe --deny-untracked-fallback-c"}),
    );
    phase_modes.insert(
        "impl_09_math_and_aux_dsos".to_string(),
        json!({"strongest_mode": "--deny-unreviewed-unsafe --deny-untracked-fallback-c"}),
    );
    phase_modes.insert(
        "impl_10_final_fixup_and_audit".to_string(),
        json!({"strongest_mode": "--deny-unreviewed-unsafe --deny-untracked-fallback-c --deny-shipped-temporary-fallback-binaries --require-cve-disposition --require-package-scope-clean"}),
    );

    SafetyPolicyToml {
        metadata: json!({
            "phase": "impl_01_safe_bootstrap",
            "purpose": "Authoritative policy schema for unsafe Rust reviews, tracked non-Rust fallbacks, and cross-manifest safety validation."
        }),
        phases: PHASE_IDS.iter().map(|phase| phase.to_string()).collect(),
        reviewed_unsafe: json!({
            "required_fields": ["site_id", "path", "line", "rationale", "reviewer", "owning_phase"],
            "notes": "Later phases populate reviewed unsafe sites; phase 1 only establishes the schema and cross-reference rules."
        }),
        reviewed_fallback: json!({
            "required_fields": ["path", "classification", "owning_phase", "shipped", "audit_notes", "reviewer"],
            "notes": "Every tracked non-Rust fallback or assembly shim must be represented in fallback-c-inventory.json and reviewed here once introduced."
        }),
        policy: json!({
            "allowed_cve_statuses": ["open", "mitigated", "not-applicable"],
            "forbid_shipped_temporary_fallback_binaries": true,
            "deny_shipped_temporary_fallback_binary_by_phase": "impl_10_final_fixup_and_audit",
            "cross_reference_files": [
                "safe/generated/baseline/fallback-c-inventory.json",
                "safe/upstream-compat/package-scope.toml",
                "safe/upstream-compat/cve-status.toml",
                "safe/generated/security/relevant-cves-index.json"
            ]
        }),
        phase_modes,
    }
}

fn build_port_status(abi_baselines: &BTreeMap<String, AbiBaseline>) -> PortStatusToml {
    let mut dso_targets = Vec::new();
    for (filename, baseline) in abi_baselines {
        let owner_phase = dso_owner_phase(&baseline.dso_id);
        let component = dso_component(&baseline.dso_id);
        dso_targets.push(DsoStatus {
            dso_id: baseline.dso_id.clone(),
            owner_phase: owner_phase.to_string(),
            abi_baseline: format!("safe/generated/baseline/abi/{filename}"),
            component: component.to_string(),
            status: "baseline_captured".to_string(),
        });
    }
    dso_targets.sort_by(|left, right| left.dso_id.cmp(&right.dso_id));

    let subsystems = vec![
        subsystem_status("support", "impl_02_hybrid_abi_shell"),
        subsystem_status("packaging", "impl_03_packaging_and_harness"),
        subsystem_status("elf", "impl_04_loader_startup_secure_exec"),
        subsystem_status("csu", "impl_04_loader_startup_secure_exec"),
        subsystem_status("sysdeps-x86_64", "impl_04_loader_startup_secure_exec"),
        subsystem_status("runtime", "impl_05_core_runtime_threads_entropy"),
        subsystem_status("threads", "impl_05_core_runtime_threads_entropy"),
        subsystem_status("entropy", "impl_05_core_runtime_threads_entropy"),
        subsystem_status("io-string-stdio", "impl_06_io_stdio_string_path"),
        subsystem_status("nss-resolver-nscd", "impl_07_nss_resolver_nscd"),
        subsystem_status("locale-iconv-posix", "impl_08_locale_iconv_posix_parsers"),
        subsystem_status("math-aux-dsos", "impl_09_math_and_aux_dsos"),
        subsystem_status("final-audit", "impl_10_final_fixup_and_audit"),
    ];

    let package_components = vec![
        json!({"package": "libc6", "status": "baseline_captured"}),
        json!({"package": "libc-bin", "status": "baseline_captured"}),
        json!({"package": "libc6-dev", "status": "baseline_captured"}),
        json!({"package": "libc-dev-bin", "status": "baseline_captured"}),
        json!({"package": "locales", "status": "baseline_captured"}),
        json!({"package": "nscd", "status": "baseline_captured"}),
        json!({"package": "libc6-dbg", "status": "baseline_captured"}),
    ];

    PortStatusToml {
        metadata: json!({
            "phase": "impl_01_safe_bootstrap",
            "notes": [
                "Status values remain baseline_captured until later implementation phases replace the relevant components under safe/**.",
                "All check-abi --all-dsos targets are seeded here so later phases consume a single committed status ledger."
            ]
        }),
        dso_targets,
        subsystems,
        package_components,
    }
}

fn collect_abi_baselines(
    context: &BaselineContext,
    package_entries: &[PackageEntry],
) -> Result<BTreeMap<String, AbiBaseline>> {
    let targets = abi_targets();
    let mut installed_paths_by_id: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for entry in package_entries {
        match entry.path.as_str() {
            path if path.ends_with("libc.so.6") => installed_paths_by_id
                .entry("libc")
                .or_default()
                .push(path.to_string()),
            path if path.ends_with("ld-linux-x86-64.so.2") || path.ends_with("ld.so") => {
                installed_paths_by_id
                    .entry("ld.so")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libm.so") || path.ends_with("libm.so.6") => {
                installed_paths_by_id
                    .entry("libm")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libmvec.so") || path.ends_with("libmvec.so.1") => {
                installed_paths_by_id
                    .entry("libmvec")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libdl.so") || path.ends_with("libdl.so.2") => {
                installed_paths_by_id
                    .entry("libdl")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("librt.so") || path.ends_with("librt.so.1") => {
                installed_paths_by_id
                    .entry("librt")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libresolv.so") || path.ends_with("libresolv.so.2") => {
                installed_paths_by_id
                    .entry("libresolv")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libpthread.so") || path.ends_with("libpthread.so.0") => {
                installed_paths_by_id
                    .entry("libpthread")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libanl.so") || path.ends_with("libanl.so.1") => {
                installed_paths_by_id
                    .entry("libanl")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libutil.so") || path.ends_with("libutil.so.1") => {
                installed_paths_by_id
                    .entry("libutil")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libBrokenLocale.so")
                || path.ends_with("libBrokenLocale.so.1") =>
            {
                installed_paths_by_id
                    .entry("libBrokenLocale")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libthread_db.so") || path.ends_with("libthread_db.so.1") => {
                installed_paths_by_id
                    .entry("libthread_db")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libnsl.so") || path.ends_with("libnsl.so.1") => {
                installed_paths_by_id
                    .entry("libnsl")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libnss_compat.so") || path.ends_with("libnss_compat.so.2") => {
                installed_paths_by_id
                    .entry("libnss_compat")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libnss_files.so") || path.ends_with("libnss_files.so.2") => {
                installed_paths_by_id
                    .entry("libnss_files")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libnss_dns.so") || path.ends_with("libnss_dns.so.2") => {
                installed_paths_by_id
                    .entry("libnss_dns")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libnss_hesiod.so") || path.ends_with("libnss_hesiod.so.2") => {
                installed_paths_by_id
                    .entry("libnss_hesiod")
                    .or_default()
                    .push(path.to_string())
            }
            path if path.ends_with("libc_malloc_debug.so.0") => installed_paths_by_id
                .entry("libc_malloc_debug")
                .or_default()
                .push(path.to_string()),
            path if path.ends_with("libmemusage.so") => installed_paths_by_id
                .entry("libmemusage")
                .or_default()
                .push(path.to_string()),
            path if path.ends_with("libpcprofile.so") => installed_paths_by_id
                .entry("libpcprofile")
                .or_default()
                .push(path.to_string()),
            _ => {}
        }
    }

    let mut baselines = BTreeMap::new();
    for target in targets {
        let primary = context.repo_root.join(target.primary_oracle);
        let dynsyms = run_command("readelf", &["--dyn-syms", "-W", &primary.to_string_lossy()])?;
        let dynamic = run_command("readelf", &["-d", "-W", &primary.to_string_lossy()])?;
        let notes = run_command("readelf", &["-n", "-W", &primary.to_string_lossy()])?;
        let exported_symbols = parse_dynamic_symbols(&dynsyms);
        let (soname, needed) = parse_dynamic_section(&dynamic);
        let build_id = parse_build_id(&notes);
        let mut map_files = Vec::new();
        for map in target.map_files {
            let path = context.repo_root.join(map);
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            map_files.push(parse_map_file(map, &contents));
        }
        let mut symlist_files = Vec::new();
        for symlist in target.symlist_files {
            let path = context.repo_root.join(symlist);
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            symlist_files.push(parse_symlist_file(symlist, &contents));
        }
        baselines.insert(
            format!("{}.json", target.dso_id),
            AbiBaseline {
                dso_id: target.dso_id.to_string(),
                primary_oracle: target.primary_oracle.to_string(),
                auxiliary_oracles: target
                    .auxiliary_oracles
                    .iter()
                    .map(|path| path.to_string())
                    .collect(),
                installed_paths: installed_paths_by_id
                    .remove(target.dso_id)
                    .unwrap_or_default(),
                build_id,
                soname,
                needed,
                exported_symbols,
                map_files,
                symlist_files,
            },
        );
    }
    Ok(baselines)
}

fn collect_test_results(context: &BaselineContext) -> Result<TestResults> {
    let mut files = Vec::new();
    let mut global_counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in sum_files(context)? {
        let contents = fs::read_to_string(context.repo_root.join(&path))
            .with_context(|| format!("failed to read {path}"))?;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries = Vec::new();
        for line in contents.lines() {
            if let Some((status, display_name)) = parse_sum_line(line) {
                *counts.entry(status.to_string()).or_insert(0) += 1;
                *global_counts.entry(status.to_string()).or_insert(0) += 1;
                let key = if display_name.contains('/') {
                    display_name.to_string()
                } else {
                    format!("top-level/{display_name}")
                };
                entries.push(SumEntry {
                    status: status.to_string(),
                    key,
                    display_name: display_name.to_string(),
                });
            }
        }
        files.push(SumFileRecord {
            path,
            counts,
            entries,
        });
    }

    Ok(TestResults {
        metadata: json!({
            "phase": "impl_01_safe_bootstrap",
            "generated_from": [
                "build/tests.sum",
                "build/subdir-tests.sum",
                "build/*/subdir-tests.sum"
            ]
        }),
        global_counts,
        sum_files: files,
    })
}

fn build_source_index(context: &BaselineContext) -> Result<SourceIndex> {
    let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_files = Vec::new();

    for entry in WalkDir::new(&context.source_root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_dir() {
            if path.file_name() == Some(OsStr::new(".pc")) {
                continue;
            }
            continue;
        }
        if path
            .components()
            .any(|component| component.as_os_str() == OsStr::new(".pc"))
        {
            continue;
        }
        let rel = repo_rel(context, path)?;
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_string();
        all_files.push(rel.clone());
        by_stem.entry(stem).or_default().push(rel);
    }

    Ok(SourceIndex { by_stem, all_files })
}

fn query_test_families(
    context: &BaselineContext,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<String>>>> {
    let mut queried = BTreeMap::new();
    queried.insert(
        "top-level".to_string(),
        query_single_makefile(&context.source_root, &context.build_root)?,
    );

    for subdir in test_subdirs(context)? {
        let dir = context.source_root.join(&subdir);
        queried.insert(subdir, query_single_makefile(&dir, &context.build_root)?);
    }
    Ok(queried)
}

fn build_test_catalog_and_plan(
    context: &BaselineContext,
    queried: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
    test_results: &TestResults,
    source_index: &SourceIndex,
) -> Result<(TestCatalog, TestPortPlan)> {
    let result_lookup = build_result_lookup(test_results);
    let makefile_cache = load_makefile_cache(context)?;
    let mut entries = Vec::new();
    let mut seen_catalog_ids = HashSet::new();
    let mut zero_entry_subdirs = Vec::new();

    for (subdir, families) in queried {
        let mut subdir_entry_count = 0usize;
        for family in TEST_FAMILIES {
            let items = families.get(family).cloned().unwrap_or_default();
            for raw_item in items {
                let normalized_name = normalize_catalog_name(subdir, &raw_item);
                let (catalog_name, variant) = materialize_variant(family, &normalized_name);
                let catalog_subdir = if subdir == "top-level" {
                    "top-level".to_string()
                } else {
                    subdir.clone()
                };
                let catalog_id = format!("{family}::{catalog_subdir}::{catalog_name}::{variant}");
                if !seen_catalog_ids.insert(catalog_id.clone()) {
                    continue;
                }
                let key = result_lookup_key(&catalog_subdir, &catalog_name);
                let origin_makefiles = contributing_makefiles(
                    context,
                    &makefile_cache,
                    &catalog_subdir,
                    &normalized_name,
                    &catalog_name,
                );
                entries.push(TestCatalogEntry {
                    catalog_id,
                    subdir: catalog_subdir.clone(),
                    name: catalog_name,
                    family: family.to_string(),
                    origin_selector: family.to_string(),
                    variant,
                    has_checked_in_baseline_result: result_lookup.contains_key(&key),
                    requires_container_or_privileged_execution: family == "tests-container",
                    origin_makefiles,
                });
                subdir_entry_count += 1;
            }
        }

        if subdir != "top-level" && subdir_entry_count == 0 {
            zero_entry_subdirs.push(ZeroEntrySubdir {
                subdir: subdir.clone(),
                owner_phase: owner_phase_for_subdir(subdir, None).to_string(),
                status: "no_executable_tests".to_string(),
                destination_root: format!("safe/tests/{subdir}"),
            });
        }
    }

    entries.sort_by(|left, right| left.catalog_id.cmp(&right.catalog_id));
    zero_entry_subdirs.sort_by(|left, right| left.subdir.cmp(&right.subdir));

    let mut plan_entries = Vec::new();
    for entry in &entries {
        let owner_phase = owner_phase_for_catalog_entry(entry);
        let (source_path, destination_path, companion_assets) =
            plan_paths_for_entry(context, source_index, entry)?;
        plan_entries.push(PortPlanEntry {
            catalog_id: entry.catalog_id.clone(),
            owner_phase: owner_phase.to_string(),
            destination_path,
            source_path,
            companion_assets,
            status: "planned".to_string(),
        });
    }
    plan_entries.sort_by(|left, right| left.catalog_id.cmp(&right.catalog_id));

    let support_root = context.source_root.join("support");
    let support_count = WalkDir::new(&support_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .count();
    let support_subtree = SupportSubtree {
        owner_phase: "impl_02_hybrid_abi_shell".to_string(),
        source_root: "original/support".to_string(),
        destination_root: "safe/tests/support".to_string(),
        asset_count: support_count,
    };

    Ok((
        TestCatalog {
            metadata: json!({
                "phase": "impl_01_safe_bootstrap",
                "families": TEST_FAMILIES,
                "full_original_test_corpus": true
            }),
            entries,
            no_executable_subdirs: zero_entry_subdirs.clone(),
        },
        TestPortPlan {
            metadata: json!({
                "phase": "impl_01_safe_bootstrap",
                "notes": [
                    "Every catalog entry is assigned to exactly one owning implementation phase.",
                    "Phase 10 is intentionally absent from entry ownership and remains fix-up only."
                ]
            }),
            entries: plan_entries,
            support_subtree,
            zero_entry_subdirs,
        },
    ))
}

fn build_fallback_inventory(
    context: &BaselineContext,
    port_plan: &TestPortPlan,
    package_entries: &[PackageEntry],
) -> Result<FallbackInventory> {
    let mut by_path: BTreeMap<String, FallbackInventoryEntry> = BTreeMap::new();

    for entry in &port_plan.entries {
        if let Some(source_path) = &entry.source_path {
            track_fallback_entry(
                &mut by_path,
                FallbackInventoryEntry {
                    path: entry.destination_path.clone(),
                    source_path: Some(source_path.clone()),
                    classification: fallback_classification(source_path),
                    owning_phase: entry.owner_phase.clone(),
                    shipped: false,
                    package_scope_refs: Vec::new(),
                    audit_notes:
                        "Planned safe-side port target derived from the phase-1 test ownership map."
                            .to_string(),
                },
            );
        }
        for companion in &entry.companion_assets {
            track_fallback_entry(
                &mut by_path,
                FallbackInventoryEntry {
                    path: companion.clone(),
                    source_path: Some(companion_to_original(companion)),
                    classification: fallback_classification(companion),
                    owning_phase: entry.owner_phase.clone(),
                    shipped: false,
                    package_scope_refs: Vec::new(),
                    audit_notes:
                        "Companion fixture or helper asset required by the owning test entry."
                            .to_string(),
                },
            );
        }
    }

    for file in WalkDir::new(context.source_root.join("support"))
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !file.file_type().is_file() {
            continue;
        }
        let rel = repo_rel(context, file.path())?;
        let destination = format!("safe/tests/{}", rel.trim_start_matches("original/"));
        track_fallback_entry(
            &mut by_path,
            FallbackInventoryEntry {
                path: destination,
                source_path: Some(rel),
                classification: fallback_classification(file.path().to_string_lossy().as_ref()),
                owning_phase: "impl_02_hybrid_abi_shell".to_string(),
                shipped: false,
                package_scope_refs: Vec::new(),
                audit_notes: "Phase 2 owns the full support subtree baseline.".to_string(),
            },
        );
    }

    for (path, refs) in packaging_assets(context)? {
        track_fallback_entry(
            &mut by_path,
            FallbackInventoryEntry {
                path,
                source_path: Some(refs.0),
                classification: refs.1,
                owning_phase: "impl_03_packaging_and_harness".to_string(),
                shipped: !refs.2.is_empty(),
                package_scope_refs: refs.2,
                audit_notes: "Committed packaging-side script, template, or data asset reserved for phase 3 and later package builds.".to_string(),
            },
        );
    }

    for entry in package_entries {
        if !matches!(
            entry.asset_kind.as_str(),
            "asm_shim" | "temporary_fallback_binary"
        ) {
            continue;
        }
        if entry.scope != "required_package" {
            continue;
        }
        let owning_phase = fallback_package_owner_phase(entry)?;
        let audit_notes = match entry.asset_kind.as_str() {
            "asm_shim" => {
                "Required-package startup or assembly shim carried by the safe port surface."
            }
            "temporary_fallback_binary" => {
                "Required-package fallback binary status tracked from the amd64-effective package manifest."
            }
            _ => unreachable!(),
        };
        track_fallback_entry(
            &mut by_path,
            FallbackInventoryEntry {
                path: entry.path.clone(),
                source_path: entry.source_path.clone(),
                classification: entry.asset_kind.clone(),
                owning_phase,
                shipped: entry.shipped_status == "shipped",
                package_scope_refs: vec![entry.path.clone()],
                audit_notes: audit_notes.to_string(),
            },
        );
    }

    Ok(FallbackInventory {
        metadata: json!({
            "phase": "impl_01_safe_bootstrap",
            "notes": [
                "This inventory is the single committed ledger of non-Rust source, script, assembly, template, and fallback assets planned under safe/**.",
                "Later phases must update this file in place instead of maintaining ad hoc fallback lists."
            ]
        }),
        entries: by_path.into_values().collect(),
    })
}

fn build_relevant_cve_index(cves: &JsonValue) -> Result<JsonValue> {
    let relevant = cves
        .get("relevant_cves")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("relevant_cves.json is missing relevant_cves"))?;
    let indexed: Vec<JsonValue> = relevant
        .iter()
        .map(|entry| {
            json!({
                "id": entry.get("id").and_then(JsonValue::as_str).unwrap_or_default(),
                "component": entry.get("component").and_then(JsonValue::as_str).unwrap_or_default(),
                "confidence": entry.get("confidence").and_then(JsonValue::as_str).unwrap_or_default(),
                "ubuntu_tracker_path": entry.get("ubuntu_tracker_path").and_then(JsonValue::as_str).unwrap_or_default()
            })
        })
        .collect();
    Ok(json!({
        "metadata": cves.get("metadata").cloned().unwrap_or_else(|| json!({})),
        "cves": indexed
    }))
}

fn build_readme(
    package_entries: &[PackageEntry],
    catalog: &TestCatalog,
    cve_count: usize,
    dependent_count: usize,
) -> String {
    let required_package_count = package_entries
        .iter()
        .filter(|entry| entry.scope == "required_package" && entry.shipped_status == "shipped")
        .count();
    let catalog_count = catalog.entries.len();
    format!(
        "# safe\n\n\
This workspace is the committed baseline capture for `impl_01_safe_bootstrap`.\n\n\
## Commands\n\n\
- `cargo run -p xtask -- ingest-baseline --source ../original --build work/original-build --dependents ../dependents.json --cves ../relevant_cves.json --verify`\n\
- `cargo run -p xtask -- audit-safety --verify-policy`\n\n\
## Generated Baselines\n\n\
- `{catalog_count}` cataloged upstream test entries keyed by stable `catalog_id`\n\
- `{required_package_count}` required-package file records plus deferred and testroot-only classifications\n\
- `{cve_count}` tracked security entries in `safe/upstream-compat/cve-status.toml`\n\
- `{dependent_count}` checked-in dependents validated in place from `dependents.json`\n\n\
## Phase Notes\n\n\
- Original repository inputs under `original/**`, `safe/work/original-build/**`, `dependents.json`, and the CVE manifests remain authoritative.\n\
- `safe/generated/baseline/test-port-plan.json` is the committed ownership map for later test porting work.\n\
- `safe/generated/baseline/fallback-c-inventory.json` and `safe/upstream-compat/safety-policy.toml` are the single safety-audit ledgers that later phases must extend in place.\n"
    )
}

fn build_result_lookup(test_results: &TestResults) -> HashMap<String, Vec<String>> {
    let mut lookup = HashMap::new();
    for file in &test_results.sum_files {
        for entry in &file.entries {
            lookup
                .entry(entry.key.clone())
                .or_insert_with(Vec::new)
                .push(entry.status.clone());
        }
    }
    lookup
}

fn load_makefile_cache(context: &BaselineContext) -> Result<BTreeMap<String, String>> {
    let mut cache = BTreeMap::new();
    let explicit = [
        "original/Makefile",
        "original/sysdeps/x86_64/Makefile",
        "original/sysdeps/x86_64/64/Makefile",
        "original/sysdeps/unix/sysv/linux/Makefile",
        "original/sysdeps/unix/sysv/linux/x86_64/Makefile",
        "original/sysdeps/unix/sysv/linux/x86_64/64/Makefile",
    ];
    for path in explicit {
        let absolute = context.repo_root.join(path);
        if absolute.exists() {
            cache.insert(path.to_string(), fs::read_to_string(absolute)?);
        }
    }

    for subdir in test_subdirs(context)? {
        let rel = format!("original/{subdir}/Makefile");
        let absolute = context.repo_root.join(&rel);
        if absolute.exists() {
            cache.entry(rel).or_insert(fs::read_to_string(absolute)?);
        }
    }
    Ok(cache)
}

fn contributing_makefiles(
    context: &BaselineContext,
    cache: &BTreeMap<String, String>,
    subdir: &str,
    normalized_name: &str,
    catalog_name: &str,
) -> Vec<String> {
    let mut matches = Vec::new();
    let base_makefile = if subdir == "top-level" {
        "original/Makefile".to_string()
    } else {
        format!("original/{subdir}/Makefile")
    };
    if cache.contains_key(&base_makefile) {
        matches.push(base_makefile);
    }

    let tokens = [normalized_name, catalog_name];
    for (path, contents) in cache {
        if path.starts_with(&format!("original/{subdir}/"))
            || path == &format!("original/{subdir}/Makefile")
        {
            continue;
        }
        if tokens
            .iter()
            .any(|token| !token.is_empty() && contents.contains(token))
        {
            matches.push(path.clone());
        }
    }
    matches.sort();
    matches.dedup();
    let _ = context;
    matches
}

fn owner_phase_for_catalog_entry(entry: &TestCatalogEntry) -> &'static str {
    if entry.subdir == "top-level"
        || entry.subdir == "support"
        || PHASE_02_REPRESENTATIVES.contains(&entry.name.as_str())
    {
        return "impl_02_hybrid_abi_shell";
    }
    owner_phase_for_subdir(&entry.subdir, Some(&entry.name))
}

fn owner_phase_for_subdir(subdir: &str, name: Option<&str>) -> &'static str {
    match subdir {
        "top-level" | "support" => "impl_02_hybrid_abi_shell",
        "elf" | "csu" => "impl_04_loader_startup_secure_exec",
        "nptl" | "nptl_db" | "malloc" | "signal" | "setjmp" | "misc" => {
            "impl_05_core_runtime_threads_entropy"
        }
        "stdlib" => {
            if let Some(name) = name {
                if name.contains("getrandom")
                    || name.contains("getentropy")
                    || name.contains("arc4random")
                {
                    return "impl_05_core_runtime_threads_entropy";
                }
            }
            "impl_06_io_stdio_string_path"
        }
        "string" | "io" | "stdio-common" | "libio" | "dirent" | "time" | "timezone" | "assert"
        | "ctype" | "termios" => "impl_06_io_stdio_string_path",
        "resolv" | "nss" | "inet" | "socket" | "nscd" | "nis" | "hesiod" => {
            "impl_07_nss_resolver_nscd"
        }
        "iconv" | "iconvdata" | "locale" | "localedata" | "posix" | "conform" | "po" => {
            "impl_08_locale_iconv_posix_parsers"
        }
        "math" | "mathvec" | "dlfcn" | "rt" | "resource" | "sysvipc" | "login" | "sunrpc"
        | "intl" | "catgets" | "wcsmbs" | "wctype" | "gmon" | "debug" | "argp" | "gnulib"
        | "manual" => "impl_09_math_and_aux_dsos",
        _ => "impl_09_math_and_aux_dsos",
    }
}

fn plan_paths_for_entry(
    context: &BaselineContext,
    source_index: &SourceIndex,
    entry: &TestCatalogEntry,
) -> Result<(Option<String>, String, Vec<String>)> {
    if entry.subdir == "top-level" {
        if let Some((_, primary, companions)) = TOP_LEVEL_SPECIALS
            .iter()
            .find(|(name, _, _)| *name == entry.name)
        {
            let source_path = primary.to_string();
            let destination = mirror_to_safe_tests(primary);
            let companion_assets = companions
                .iter()
                .map(|path| mirror_to_safe_tests(path))
                .collect();
            return Ok((Some(source_path), destination, companion_assets));
        }
    }

    if entry.name.contains('/') {
        let direct_source = format!("original/{}/{}", entry.subdir, entry.name);
        if context.repo_root.join(&direct_source).exists() {
            return Ok((
                Some(direct_source.clone()),
                mirror_to_safe_tests(&direct_source),
                Vec::new(),
            ));
        }
    }

    let base_name = strip_generated_suffix(&entry.name);
    let preferred_roots = preferred_source_roots(entry);
    let primary = find_primary_source(source_index, &preferred_roots, &base_name, &entry.name);

    if let Some(source_path) = primary {
        let destination = mirror_to_safe_tests(&source_path);
        let companion_assets =
            find_companion_assets(source_index, &source_path, &base_name, &entry.name)
                .into_iter()
                .map(|path| mirror_to_safe_tests(&path))
                .collect();
        return Ok((Some(source_path), destination, companion_assets));
    }

    Ok((
        None,
        format!("safe/tests/{}/{}", entry.subdir, entry.name),
        Vec::new(),
    ))
}

fn preferred_source_roots(entry: &TestCatalogEntry) -> Vec<String> {
    let mut roots = Vec::new();
    if entry.subdir != "top-level" {
        roots.push(format!("original/{}", entry.subdir));
    }
    for makefile in &entry.origin_makefiles {
        let Some(parent) = Path::new(makefile).parent() else {
            continue;
        };
        let root = parent.to_string_lossy().replace('\\', "/");
        if root == "original" || roots.iter().any(|existing| existing == &root) {
            continue;
        }
        roots.push(root);
    }
    roots
}

fn find_primary_source(
    source_index: &SourceIndex,
    preferred_roots: &[String],
    base_name: &str,
    full_name: &str,
) -> Option<String> {
    if let Some(candidates) = source_index.by_stem.get(base_name) {
        if let Some(path) = choose_path(candidates, preferred_roots) {
            return Some(path);
        }
    }
    if let Some(candidates) = source_index.by_stem.get(full_name) {
        if let Some(path) = choose_path(candidates, preferred_roots) {
            return Some(path);
        }
    }

    for extension in ["c", "cc", "cpp", "cxx", "S", "s", "sh", "py", "pl", "awk"] {
        for root in preferred_roots {
            let candidate = format!("{root}/{base_name}.{extension}");
            if source_index.all_files.iter().any(|path| path == &candidate) {
                return Some(candidate);
            }
            let candidate = format!("{root}/{full_name}.{extension}");
            if source_index.all_files.iter().any(|path| path == &candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn choose_path(candidates: &[String], preferred_roots: &[String]) -> Option<String> {
    for root in preferred_roots {
        if let Some(path) = candidates
            .iter()
            .find(|path| path.starts_with(root))
            .cloned()
        {
            return Some(path);
        }
    }
    candidates.first().cloned()
}

fn find_companion_assets(
    source_index: &SourceIndex,
    primary: &str,
    base_name: &str,
    full_name: &str,
) -> Vec<String> {
    let directory = Path::new(primary)
        .parent()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let mut companions = BTreeSet::new();
    for stem in [base_name, full_name] {
        if let Some(candidates) = source_index.by_stem.get(stem) {
            for candidate in candidates {
                if candidate != primary && candidate.starts_with(&directory) {
                    companions.insert(candidate.clone());
                }
            }
        }
    }
    companions.into_iter().collect()
}

fn mirror_to_safe_tests(original_path: &str) -> String {
    format!(
        "safe/tests/{}",
        original_path.trim_start_matches("original/")
    )
}

fn companion_to_original(safe_path: &str) -> String {
    format!("original/{}", safe_path.trim_start_matches("safe/tests/"))
}

fn fallback_package_owner_phase(entry: &PackageEntry) -> Result<String> {
    if let Some(owner_phase) = &entry.owner_phase {
        return Ok(owner_phase.clone());
    }
    match entry.path.as_str() {
        "/usr/lib/pt_chown" => Ok("impl_05_core_runtime_threads_entropy".to_string()),
        path if entry.asset_kind == "asm_shim" && is_startup_object(path) => {
            Ok("impl_04_loader_startup_secure_exec".to_string())
        }
        _ => bail!(
            "missing fallback inventory owner mapping for package asset {} ({})",
            entry.path,
            entry.asset_kind
        ),
    }
}

fn track_fallback_entry(
    entries: &mut BTreeMap<String, FallbackInventoryEntry>,
    entry: FallbackInventoryEntry,
) {
    entries.entry(entry.path.clone()).or_insert(entry);
}

fn packaging_assets(
    context: &BaselineContext,
) -> Result<Vec<(String, (String, String, Vec<String>))>> {
    let mut assets = Vec::new();
    for entry in WalkDir::new(context.source_root.join("debian/local"))
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = repo_rel(context, entry.path())?;
        let local_rel = rel.trim_start_matches("original/debian/local/").to_string();
        let safe_path = format!("safe/debian/local/{local_rel}");
        let package_refs = packaging_refs_for_local_path(&local_rel);
        assets.push((
            safe_path,
            (rel, fallback_classification(&local_rel), package_refs),
        ));
    }

    for entry in WalkDir::new(context.source_root.join("debian/script.in"))
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = repo_rel(context, entry.path())?;
        let local_rel = rel.trim_start_matches("original/debian/").to_string();
        let safe_path = format!("safe/debian/{local_rel}");
        assets.push((
            safe_path,
            (rel, fallback_classification(&local_rel), Vec::new()),
        ));
    }

    for entry in WalkDir::new(context.source_root.join("debian/debhelper.in"))
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = repo_rel(context, entry.path())?;
        let local_rel = rel.trim_start_matches("original/debian/");
        let safe_path = format!("safe/debian/{local_rel}");
        assets.push((safe_path, (rel, "template_asset".to_string(), Vec::new())));
    }

    for special in ["nscd.service", "nscd.tmpfiles"] {
        let rel = format!("original/nscd/{special}");
        let source = context.repo_root.join(&rel);
        if source.exists() {
            assets.push((
                format!("safe/debian/{special}"),
                (rel, "data_asset".to_string(), Vec::new()),
            ));
        }
    }

    Ok(assets)
}

fn helper_record(
    path: &str,
    source_manifest: &str,
    shipped_status: &str,
    verification: &str,
    owner_phase: Option<&str>,
) -> HelperPathRecord {
    HelperPathRecord {
        path: path.to_string(),
        source_manifest: source_manifest.to_string(),
        shipped_status: shipped_status.to_string(),
        verification: verification.to_string(),
        owner_phase: owner_phase.map(str::to_string),
    }
}

fn helper_shipped_status(entries: &[PackageEntry], path: &str, default_status: &str) -> String {
    entries
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.shipped_status.clone())
        .unwrap_or_else(|| default_status.to_string())
}

fn package_for_entrypoint(path: &str) -> &'static str {
    match path {
        "/usr/sbin/nscd" => "nscd",
        "/usr/sbin/locale-gen"
        | "/usr/sbin/update-locale"
        | "/usr/sbin/validlocale"
        | "/usr/share/locales/install-language-pack"
        | "/usr/share/locales/remove-language-pack" => "locales",
        "/usr/bin/gencat" => "libc-dev-bin",
        _ => "libc-bin",
    }
}

fn classify_asset_kind(path: PathBuf, logical_path: &str, executable: bool) -> String {
    let basename = Path::new(logical_path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(logical_path);
    if logical_path.contains("/usr/lib/debug/") || logical_path.ends_with(".debug") {
        return "debug_asset".to_string();
    }
    if looks_like_script(&path) {
        return "script_asset".to_string();
    }
    if logical_path.ends_with(".o")
        || logical_path.ends_with(".S")
        || logical_path.ends_with(".s")
        || logical_path.contains("/crt")
    {
        return "asm_shim".to_string();
    }
    if is_elf(&path)
        || basename.ends_with(".so")
        || basename == "ld.so"
        || basename
            .split_once(".so.")
            .map(|(_, suffix)| suffix.chars().all(|ch| ch.is_ascii_digit() || ch == '.'))
            .unwrap_or(false)
    {
        return "rust_target".to_string();
    }
    if executable {
        return "script_asset".to_string();
    }
    "data_asset".to_string()
}

fn derive_libc6_dbg_entries(
    context: &BaselineContext,
    entries: &BTreeMap<String, PackageEntry>,
) -> Result<Vec<PackageEntry>> {
    let mut derived = Vec::new();
    for entry in entries.values() {
        if entry.package != "libc6" {
            continue;
        }
        let Some(source_path) = &entry.source_path else {
            continue;
        };
        let source_abs = context.repo_root.join(source_path);
        if !is_elf(&source_abs) {
            continue;
        }
        let build_id = read_elf_build_id(&source_abs)?;
        let Some(build_id) = build_id else {
            continue;
        };
        let debug_path = format!(
            "/usr/lib/debug/.build-id/{}/{}.debug",
            &build_id[..2],
            &build_id[2..]
        );
        derived.push(PackageEntry {
            package: "libc6-dbg".to_string(),
            path: debug_path,
            source_path: Some(entry.path.clone()),
            source_origin: "derived_debug".to_string(),
            scope: "required_package".to_string(),
            shipped_status: "shipped".to_string(),
            asset_kind: "debug_asset".to_string(),
            executable: entry.executable,
            symlink_target: None,
            owner_phase: None,
            verification: Some("basic-required-packages".to_string()),
        });
    }
    derived.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(derived)
}

fn read_elf_build_id(path: &Path) -> Result<Option<String>> {
    let output = run_command("readelf", &["-n", "-W", &path.to_string_lossy()])?;
    Ok(parse_build_id(&output))
}

fn fallback_classification(path: &str) -> String {
    if path.ends_with(".S") || path.ends_with(".s") || path.ends_with(".sym") {
        return "asm_shim".to_string();
    }
    if path.ends_with(".sh")
        || path.ends_with(".py")
        || path.ends_with(".pl")
        || path.ends_with(".awk")
        || path.ends_with("/ldconfig")
    {
        return "script_asset".to_string();
    }
    if path.ends_with(".install")
        || path.ends_with(".manpages")
        || path.ends_with(".dirs")
        || path.ends_with(".links")
        || path.ends_with(".postinst")
        || path.ends_with(".postrm")
        || path.ends_with(".preinst")
        || path.ends_with(".prerm")
        || path.ends_with(".templates")
        || path.ends_with(".config")
        || path.ends_with(".README.Debian")
        || path.ends_with(".NEWS")
    {
        return "template_asset".to_string();
    }
    if path.ends_with(".c")
        || path.ends_with(".cc")
        || path.ends_with(".cpp")
        || path.ends_with(".cxx")
        || path.ends_with(".h")
        || path.ends_with(".inc")
        || path.ends_with(".lds")
    {
        return "non_rust_source".to_string();
    }
    "data_asset".to_string()
}

fn packaging_refs_for_local_path(local_rel: &str) -> Vec<String> {
    match local_rel {
        "etc/bindresvport.blacklist" => vec!["/etc/bindresvport.blacklist".to_string()],
        "etc/ld.so.conf" => vec!["/etc/ld.so.conf".to_string()],
        "etc/ld.so.conf.d/libc.conf" => vec!["/etc/ld.so.conf.d/libc.conf".to_string()],
        "etc/nsswitch.conf" => vec!["/usr/share/libc-bin/nsswitch.conf".to_string()],
        "sbin/ldconfig" => vec!["/usr/sbin/ldconfig".to_string()],
        "usr_sbin/locale-gen" => vec!["/usr/sbin/locale-gen".to_string()],
        "usr_sbin/update-locale" => vec!["/usr/sbin/update-locale".to_string()],
        "usr_sbin/validlocale" => vec!["/usr/sbin/validlocale".to_string()],
        "usr_share_locales/install-language-pack" => {
            vec!["/usr/share/locales/install-language-pack".to_string()]
        }
        "usr_share_locales/remove-language-pack" => {
            vec!["/usr/share/locales/remove-language-pack".to_string()]
        }
        _ => Vec::new(),
    }
}

fn sum_files(context: &BaselineContext) -> Result<Vec<String>> {
    let mut files = vec![
        "build/tests.sum".to_string(),
        "build/subdir-tests.sum".to_string(),
    ];
    for entry in WalkDir::new(&context.build_root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if entry.file_type().is_file() && path.file_name() == Some(OsStr::new("subdir-tests.sum")) {
            let rel = repo_rel(context, path)?;
            if rel != "build/subdir-tests.sum" {
                files.push(rel);
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn test_subdirs(context: &BaselineContext) -> Result<Vec<String>> {
    let mut subdirs = BTreeSet::new();
    for entry in WalkDir::new(&context.build_root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !entry.file_type().is_file() || path.file_name() != Some(OsStr::new("subdir-tests.sum"))
        {
            continue;
        }
        let rel = path
            .strip_prefix(&context.build_root)
            .with_context(|| format!("failed to strip build root from {}", path.display()))?;
        if rel == Path::new("subdir-tests.sum") {
            continue;
        }
        let subdir = rel
            .parent()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if context.source_root.join(&subdir).join("Makefile").exists() {
            subdirs.insert(subdir);
        }
    }
    Ok(subdirs.into_iter().collect())
}

fn query_single_makefile(
    directory: &Path,
    build_root: &Path,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut temp = NamedTempFile::new()?;
    writeln!(temp, "include Makefile")?;
    writeln!(temp, "print-vars:")?;
    for family in TEST_FAMILIES {
        writeln!(temp, "\t@printf '{}=%s\\n' '$({})'", family, family)?;
    }

    let output = Command::new("make")
        .arg("-C")
        .arg(directory)
        .arg("-s")
        .arg("-f")
        .arg(temp.path())
        .arg(format!("objdir={}", build_root.display()))
        .arg("print-vars")
        .output()
        .with_context(|| format!("failed to run make query in {}", directory.display()))?;

    if !output.status.success() {
        bail!(
            "make query failed in {}:\n{}",
            directory.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout).context("make query output was not UTF-8")?;
    let mut families = BTreeMap::new();
    for line in stdout.lines() {
        if let Some((family, values)) = line.split_once('=') {
            let normalized = values
                .split_whitespace()
                .map(|value| normalize_make_value(build_root, family, value))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            families.insert(family.to_string(), normalized);
        }
    }
    Ok(families)
}

fn normalize_make_value(build_root: &Path, family: &str, value: &str) -> String {
    let rel = if Path::new(value).is_absolute() {
        if let Ok(stripped) = Path::new(value).strip_prefix(build_root) {
            stripped.to_string_lossy().replace('\\', "/")
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    };
    if family.ends_with("special") && rel.ends_with(".out") {
        rel.trim_end_matches(".out").to_string()
    } else {
        rel
    }
}

fn normalize_catalog_name(subdir: &str, raw_item: &str) -> String {
    if subdir == "top-level" {
        raw_item.to_string()
    } else if let Some(stripped) = raw_item.strip_prefix(&format!("{subdir}/")) {
        stripped.to_string()
    } else {
        raw_item.to_string()
    }
}

fn materialize_variant(family: &str, normalized_name: &str) -> (String, String) {
    match family {
        "tests-mcheck" => (format!("{normalized_name}-mcheck"), "mcheck".to_string()),
        "tests-malloc-check" => (
            format!("{normalized_name}-malloc-check"),
            "malloc-check".to_string(),
        ),
        "tests-malloc-hugetlb1" => (
            format!("{normalized_name}-malloc-hugetlb1"),
            "malloc-hugetlb1".to_string(),
        ),
        "tests-malloc-hugetlb2" => (
            format!("{normalized_name}-malloc-hugetlb2"),
            "malloc-hugetlb2".to_string(),
        ),
        _ => (normalized_name.to_string(), "base".to_string()),
    }
}

fn strip_generated_suffix(name: &str) -> String {
    for suffix in [
        "-mcheck",
        "-malloc-check",
        "-malloc-hugetlb1",
        "-malloc-hugetlb2",
    ] {
        if let Some(base) = name.strip_suffix(suffix) {
            return base.to_string();
        }
    }
    name.to_string()
}

fn result_lookup_key(subdir: &str, name: &str) -> String {
    if subdir == "top-level" {
        format!("top-level/{name}")
    } else {
        format!("{subdir}/{name}")
    }
}

fn parse_sum_line(line: &str) -> Option<(&str, &str)> {
    let (status, name) = line.split_once(": ")?;
    if status.chars().all(|ch| ch.is_ascii_uppercase()) {
        Some((status, name))
    } else {
        None
    }
}

fn resolve_path(current_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        let joined = current_dir.join(path);
        fs::canonicalize(&joined).unwrap_or(joined)
    }
}

fn repo_rel(context: &BaselineContext, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(&context.repo_root)
        .with_context(|| format!("failed to strip repo root from {}", path.display()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn read_json(path: &Path) -> Result<JsonValue> {
    serde_json::from_str(
        &fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let contents = serde_json::to_string_pretty(value)?;
    fs::write(path, contents)?;
    Ok(())
}

fn write_toml(path: &Path, value: &impl Serialize) -> Result<()> {
    let contents = toml::to_string_pretty(value)?;
    fs::write(path, contents)?;
    Ok(())
}

fn sha256_path(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn looks_like_script(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"#!")
}

fn is_elf(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"\x7fELF")
}

fn collect_prefixed_files(root: &Path, prefix: &str) -> Result<Vec<(String, String)>> {
    let mut entries = Vec::new();
    if !root.exists() {
        return Ok(entries);
    }
    for entry in WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .with_context(|| format!("failed to strip prefix {}", root.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        entries.push((
            format!("{prefix}/{relative}"),
            format!("{}/{}", prefix.trim_start_matches("original/"), relative),
        ));
    }
    Ok(entries)
}

fn substitute_manifest_pattern(value: &str) -> String {
    value
        .replace("usrRTLDDIR", "usr/lib64")
        .replace("usrSLIBDIR", "usr/lib64")
        .replace("RTLDDIR", "lib64")
        .replace("SLIBDIR", "lib64")
        .replace("LIBDIR", "usr/lib64")
        .replace("${env:build-tree}", "build-tree")
}

fn expand_build_patterns(pattern: &str) -> Vec<String> {
    let mut patterns = vec![pattern.to_string()];
    if pattern.contains("usr/lib/*/") {
        patterns.push(pattern.replace("usr/lib/*/", "usr/lib64/"));
    }
    patterns
}

fn matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| matches_pattern(path, pattern))
}

fn matches_pattern(path: &str, pattern: &str) -> bool {
    if path == pattern {
        return true;
    }
    if let Ok(glob) = Pattern::new(pattern) {
        if glob.matches(path) {
            return true;
        }
    }
    let normalized_pattern = pattern.trim_end_matches('/');
    if path.starts_with(normalized_pattern)
        && path
            .as_bytes()
            .get(normalized_pattern.len())
            .map(|byte| *byte == b'/')
            .unwrap_or(false)
    {
        return true;
    }
    false
}

fn apply_destination(source_pattern: &str, destination: Option<&str>, relative: &str) -> String {
    let packaged = if let Some(destination) = destination {
        let destination = destination.trim_start_matches('/');
        if source_pattern.contains('*') || source_pattern.contains('?') {
            let basename = Path::new(relative)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or(relative);
            format!("/{destination}/{basename}")
        } else if relative == source_pattern {
            let basename = Path::new(source_pattern)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or(relative);
            format!("/{destination}/{basename}")
        } else if let Some(stripped) = relative.strip_prefix(&format!("{source_pattern}/")) {
            let root_name = Path::new(source_pattern)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default();
            format!("/{destination}/{root_name}/{stripped}")
        } else {
            let basename = Path::new(relative)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or(relative);
            format!("/{destination}/{basename}")
        }
    } else {
        format!("/{}", relative.trim_start_matches('/'))
    };
    packaged.replace("//", "/")
}

fn upsert_package_entry(
    entries: &mut BTreeMap<String, PackageEntry>,
    package: &str,
    path: &str,
    source_path: Option<String>,
    source_probe: Option<PathBuf>,
    source_origin: &str,
    _manifest_package: &str,
    executable: bool,
    symlink_target: Option<String>,
) {
    let scope = match package {
        "libc6" | "libc-bin" | "libc6-dev" | "libc-dev-bin" | "locales" | "nscd" | "libc6-dbg" => {
            "required_package"
        }
        _ => "deferred_package",
    };
    let shipped_status = "shipped";
    let asset_kind = classify_asset_kind(
        source_probe.unwrap_or_else(|| PathBuf::from(path)),
        path,
        executable,
    );
    let (owner_phase, verification) = explicit_entrypoint_assignment(path);
    entries.entry(path.to_string()).or_insert(PackageEntry {
        package: package.to_string(),
        path: path.to_string(),
        source_path,
        source_origin: source_origin.to_string(),
        scope: scope.to_string(),
        shipped_status: shipped_status.to_string(),
        asset_kind,
        executable,
        symlink_target,
        owner_phase,
        verification,
    });
}

fn explicit_entrypoint_assignment(path: &str) -> (Option<String>, Option<String>) {
    match path {
        "/usr/sbin/ldconfig" | "/usr/bin/ldd" | "/usr/bin/ld.so" => (
            Some("impl_04_loader_startup_secure_exec".to_string()),
            Some("loader-tools".to_string()),
        ),
        "/usr/bin/pldd" => (
            Some("impl_05_core_runtime_threads_entropy".to_string()),
            Some("runtime-tools".to_string()),
        ),
        "/usr/bin/getent" | "/usr/sbin/nscd" => (
            Some("impl_07_nss_resolver_nscd".to_string()),
            Some("network-tools".to_string()),
        ),
        "/usr/bin/iconv"
        | "/usr/sbin/iconvconfig"
        | "/usr/bin/localedef"
        | "/usr/bin/locale"
        | "/usr/sbin/locale-gen"
        | "/usr/sbin/update-locale"
        | "/usr/sbin/validlocale"
        | "/usr/share/locales/install-language-pack"
        | "/usr/share/locales/remove-language-pack" => (
            Some("impl_08_locale_iconv_posix_parsers".to_string()),
            Some("locale-tools".to_string()),
        ),
        "/usr/bin/gencat" | "/usr/bin/getconf" | "/usr/bin/tzselect" | "/usr/bin/zdump"
        | "/usr/sbin/zic" => (
            Some("impl_09_math_and_aux_dsos".to_string()),
            Some("dev-and-time-tools".to_string()),
        ),
        _ => (None, None),
    }
}

fn is_startup_object(path: &str) -> bool {
    matches!(
        path,
        "/usr/lib64/Mcrt1.o"
            | "/usr/lib64/Scrt1.o"
            | "/usr/lib64/crt1.o"
            | "/usr/lib64/crti.o"
            | "/usr/lib64/crtn.o"
            | "/usr/lib64/gcrt1.o"
            | "/usr/lib64/grcrt1.o"
            | "/usr/lib64/rcrt1.o"
    )
}

fn run_command(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed with status {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).context("command output was not UTF-8")
}

fn parse_dynamic_symbols(output: &str) -> Vec<String> {
    let mut symbols = BTreeSet::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 8 || !fields[0].ends_with(':') {
            continue;
        }
        let ndx = fields[6];
        let name = fields[7];
        if ndx != "UND" {
            symbols.insert(name.to_string());
        }
    }
    symbols.into_iter().collect()
}

fn parse_dynamic_section(output: &str) -> (Option<String>, Vec<String>) {
    let soname_re = Regex::new(r"\(SONAME\).*?\[(?P<value>.+?)\]").unwrap();
    let needed_re = Regex::new(r"\(NEEDED\).*?\[(?P<value>.+?)\]").unwrap();
    let mut soname = None;
    let mut needed = Vec::new();
    for line in output.lines() {
        if let Some(captures) = soname_re.captures(line) {
            soname = Some(captures["value"].to_string());
        }
        if let Some(captures) = needed_re.captures(line) {
            needed.push(captures["value"].to_string());
        }
    }
    (soname, needed)
}

fn parse_build_id(output: &str) -> Option<String> {
    let build_id_re = Regex::new(r"Build ID:\s*([0-9a-fA-F]+)").unwrap();
    build_id_re
        .captures(output)
        .map(|captures| captures[1].to_lowercase())
}

fn parse_map_file(path: &str, contents: &str) -> JsonValue {
    if contents.starts_with("Archive member included") {
        return json!({
            "path": path,
            "kind": "link_map",
            "line_count": contents.lines().count(),
            "sha256": format!("{:x}", Sha256::digest(contents.as_bytes()))
        });
    }

    let mut versions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current_version: Option<String> = None;
    let mut in_global_block = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with('{') {
            current_version = Some(trimmed.trim_end_matches('{').trim().to_string());
            in_global_block = false;
            continue;
        }
        if trimmed == "global:" {
            in_global_block = true;
            continue;
        }
        if trimmed.starts_with("local:") {
            in_global_block = false;
            continue;
        }
        if in_global_block {
            for item in trimmed.split(';') {
                let symbol = item.trim();
                if symbol.is_empty() || symbol.starts_with('#') {
                    continue;
                }
                if let Some(version) = current_version.as_ref() {
                    versions
                        .entry(version.clone())
                        .or_default()
                        .push(symbol.to_string());
                }
            }
        }
    }

    json!({
        "path": path,
        "kind": "version_script",
        "versions": versions,
        "sha256": format!("{:x}", Sha256::digest(contents.as_bytes()))
    })
}

fn parse_symlist_file(path: &str, contents: &str) -> JsonValue {
    let entries: Vec<JsonValue> = contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let version = fields.next()?;
            let symbol = fields.next()?;
            let kind = fields.next().unwrap_or("");
            Some(json!({
                "version": version,
                "symbol": symbol,
                "kind": kind
            }))
        })
        .collect();

    json!({
        "path": path,
        "entries": entries,
        "sha256": format!("{:x}", Sha256::digest(contents.as_bytes()))
    })
}

fn json_string(value: Option<&JsonValue>) -> Result<String> {
    value
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("expected string value"))
}

fn subsystem_status(name: &str, owner_phase: &str) -> SubsystemStatus {
    SubsystemStatus {
        name: name.to_string(),
        owner_phase: owner_phase.to_string(),
        status: "baseline_captured".to_string(),
    }
}

fn dso_owner_phase(dso_id: &str) -> &'static str {
    match dso_id {
        "ld.so" => "impl_04_loader_startup_secure_exec",
        "libpthread" | "libthread_db" | "libc_malloc_debug" | "libmemusage" => {
            "impl_05_core_runtime_threads_entropy"
        }
        "libanl" | "libnsl" | "libnss_compat" | "libnss_files" | "libnss_dns" | "libnss_hesiod"
        | "libresolv" => "impl_07_nss_resolver_nscd",
        "libBrokenLocale" => "impl_08_locale_iconv_posix_parsers",
        "libm" | "libmvec" | "libdl" | "librt" | "libutil" | "libpcprofile" => {
            "impl_09_math_and_aux_dsos"
        }
        _ => "impl_05_core_runtime_threads_entropy",
    }
}

fn dso_component(dso_id: &str) -> &'static str {
    match dso_id {
        "ld.so" => "loader",
        "libpthread" | "libthread_db" => "threads",
        "libc_malloc_debug" | "libmemusage" => "malloc-debug",
        "libanl" | "libnsl" | "libnss_compat" | "libnss_files" | "libnss_dns" | "libnss_hesiod"
        | "libresolv" => "network-identity",
        "libBrokenLocale" => "locale",
        "libm" | "libmvec" => "math",
        "libdl" => "dlfcn",
        "librt" => "rt",
        "libutil" => "login",
        "libpcprofile" => "debug",
        _ => "core-libc",
    }
}

fn abi_targets() -> Vec<AbiTarget<'static>> {
    vec![
        AbiTarget::new(
            "libc",
            "build/libc.so.6",
            &["build/libc.so"],
            &["build/libc.map"],
            &["build/libc.symlist", "build/nptl_db/libc.symlist-private"],
        ),
        AbiTarget::new(
            "ld.so",
            "build/elf/ld.so",
            &[],
            &["build/ld.map", "build/elf/librtld.map"],
            &["build/elf/ld.symlist"],
        ),
        AbiTarget::new(
            "libm",
            "build/math/libm.so",
            &[],
            &["build/libm.map"],
            &["build/math/libm.symlist"],
        ),
        AbiTarget::new(
            "libmvec",
            "build/mathvec/libmvec.so",
            &[],
            &["build/libmvec.map"],
            &["build/mathvec/libmvec.symlist"],
        ),
        AbiTarget::new(
            "libdl",
            "build/dlfcn/libdl.so",
            &[],
            &["build/libdl.map"],
            &["build/dlfcn/libdl.symlist"],
        ),
        AbiTarget::new(
            "librt",
            "build/rt/librt.so",
            &[],
            &["build/librt.map"],
            &["build/rt/librt.symlist"],
        ),
        AbiTarget::new(
            "libresolv",
            "build/resolv/libresolv.so",
            &[],
            &["build/libresolv.map"],
            &["build/resolv/libresolv.symlist"],
        ),
        AbiTarget::new(
            "libpthread",
            "build/nptl/libpthread.so",
            &[],
            &["build/libpthread.map"],
            &["build/nptl/libpthread.symlist"],
        ),
        AbiTarget::new(
            "libanl",
            "build/resolv/libanl.so",
            &[],
            &["build/libanl.map"],
            &["build/resolv/libanl.symlist"],
        ),
        AbiTarget::new(
            "libutil",
            "build/login/libutil.so",
            &[],
            &["build/libutil.map"],
            &["build/login/libutil.symlist"],
        ),
        AbiTarget::new(
            "libBrokenLocale",
            "build/locale/libBrokenLocale.so",
            &[],
            &["build/libBrokenLocale.map"],
            &["build/locale/libBrokenLocale.symlist"],
        ),
        AbiTarget::new(
            "libthread_db",
            "build/nptl_db/libthread_db.so",
            &[],
            &["build/libthread_db.map"],
            &["build/nptl_db/libthread_db.symlist"],
        ),
        AbiTarget::new(
            "libnsl",
            "build/nis/libnsl.so",
            &[],
            &["build/libnsl.map"],
            &["build/nis/libnsl.symlist"],
        ),
        AbiTarget::new(
            "libnss_compat",
            "build/nss/libnss_compat.so",
            &[],
            &["build/libnss_compat.map"],
            &["build/nss/libnss_compat.symlist"],
        ),
        AbiTarget::new(
            "libnss_files",
            "build/nss/libnss_files.so",
            &[],
            &["build/libnss_files.map"],
            &["build/nss/libnss_files.symlist"],
        ),
        AbiTarget::new(
            "libnss_dns",
            "build/resolv/libnss_dns.so",
            &[],
            &["build/libnss_dns.map"],
            &["build/resolv/libnss_dns.symlist"],
        ),
        AbiTarget::new(
            "libnss_hesiod",
            "build/hesiod/libnss_hesiod.so",
            &[],
            &["build/libnss_hesiod.map"],
            &["build/hesiod/libnss_hesiod.symlist"],
        ),
        AbiTarget::new(
            "libc_malloc_debug",
            "build/malloc/libc_malloc_debug.so.0",
            &[],
            &["build/libc_malloc_debug.map"],
            &["build/malloc/libc_malloc_debug.symlist"],
        ),
        AbiTarget::new("libmemusage", "build/malloc/libmemusage.so", &[], &[], &[]),
        AbiTarget::new("libpcprofile", "build/debug/libpcprofile.so", &[], &[], &[]),
    ]
}

#[derive(Clone, Debug)]
struct AbiTarget<'a> {
    dso_id: &'a str,
    primary_oracle: &'a str,
    auxiliary_oracles: &'a [&'a str],
    map_files: &'a [&'a str],
    symlist_files: &'a [&'a str],
}

impl<'a> AbiTarget<'a> {
    fn new(
        dso_id: &'a str,
        primary_oracle: &'a str,
        auxiliary_oracles: &'a [&'a str],
        map_files: &'a [&'a str],
        symlist_files: &'a [&'a str],
    ) -> Self {
        Self {
            dso_id,
            primary_oracle,
            auxiliary_oracles,
            map_files,
            symlist_files,
        }
    }
}
