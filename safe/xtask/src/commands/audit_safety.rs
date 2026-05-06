use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use regex::Regex;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;
use walkdir::WalkDir;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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

const AUTO_REVIEWER: &str = "phase-5-auto-review";
const REVIEWED_UNSAFE_ENTRIES_KEY: &str = "reviewed_unsafe_entries";
const REVIEWED_FALLBACK_ENTRIES_KEY: &str = "reviewed_fallback_entries";

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub verify_policy: bool,
    #[arg(long)]
    pub deny_unreviewed_unsafe: bool,
    #[arg(long)]
    pub deny_untracked_fallback_c: bool,
    #[arg(long)]
    pub deny_shipped_temporary_fallback_binaries: bool,
    #[arg(long)]
    pub deny_shipped_private_backend_dsos: bool,
    #[arg(long)]
    pub require_cve_disposition: bool,
    #[arg(long)]
    pub require_package_scope_clean: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UnsafeSite {
    pub site_id: String,
    pub path: String,
    pub line: usize,
    pub rationale: String,
    pub reviewer: String,
    pub owning_phase: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReviewedFallback {
    pub path: String,
    pub classification: String,
    pub owning_phase: String,
    pub shipped: bool,
    pub audit_notes: String,
    pub reviewer: String,
}

struct VerificationContext {
    policy: TomlValue,
    fallback_json: JsonValue,
    package_scope: TomlValue,
    cve_status: TomlValue,
    cve_index: JsonValue,
}

pub fn run(args: Args) -> Result<()> {
    if !args.verify_policy
        && !args.deny_unreviewed_unsafe
        && !args.deny_untracked_fallback_c
        && !args.deny_shipped_temporary_fallback_binaries
        && !args.deny_shipped_private_backend_dsos
        && !args.require_cve_disposition
        && !args.require_package_scope_clean
    {
        bail!("audit-safety requires at least one explicit verification flag");
    }

    let safe_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow!("failed to locate safe workspace root"))?
        .to_path_buf();

    let context = verify_policy(&safe_root)?;
    if args.deny_unreviewed_unsafe {
        enforce_reviewed_unsafe(&safe_root, &context.policy)?;
    }
    if args.deny_untracked_fallback_c {
        enforce_reviewed_fallbacks(&safe_root, &context.policy, &context.fallback_json)?;
    }
    if args.deny_shipped_temporary_fallback_binaries {
        enforce_no_shipped_temporary_fallback_binaries(&context.fallback_json)?;
    }
    if args.deny_shipped_private_backend_dsos {
        enforce_no_shipped_private_backend_dsos(&context.package_scope, &context.fallback_json)?;
    }
    if args.require_cve_disposition {
        enforce_cve_disposition(&context.policy, &context.cve_status, &context.cve_index)?;
    }
    if args.require_package_scope_clean {
        enforce_package_scope_clean(&context.package_scope)?;
    }

    println!("audit-safety: verification passed");
    Ok(())
}

pub(crate) fn build_reviewed_unsafe_policy_entries(safe_root: &Path) -> Result<Vec<TomlValue>> {
    collect_unsafe_sites(safe_root)?
        .into_iter()
        .map(|site| {
            let mut table = toml::map::Map::new();
            table.insert("site_id".to_string(), TomlValue::String(site.site_id));
            table.insert("path".to_string(), TomlValue::String(site.path));
            table.insert("line".to_string(), TomlValue::Integer(site.line as i64));
            table.insert("rationale".to_string(), TomlValue::String(site.rationale));
            table.insert("reviewer".to_string(), TomlValue::String(site.reviewer));
            table.insert(
                "owning_phase".to_string(),
                TomlValue::String(site.owning_phase),
            );
            Ok(TomlValue::Table(table))
        })
        .collect()
}

pub(crate) fn build_reviewed_fallback_policy_entries(safe_root: &Path) -> Result<Vec<TomlValue>> {
    collect_reviewed_fallbacks(safe_root)?
        .into_iter()
        .map(|review| {
            let mut table = toml::map::Map::new();
            table.insert("path".to_string(), TomlValue::String(review.path));
            table.insert(
                "classification".to_string(),
                TomlValue::String(review.classification),
            );
            table.insert(
                "owning_phase".to_string(),
                TomlValue::String(review.owning_phase),
            );
            table.insert("shipped".to_string(), TomlValue::Boolean(review.shipped));
            table.insert(
                "audit_notes".to_string(),
                TomlValue::String(review.audit_notes),
            );
            table.insert("reviewer".to_string(), TomlValue::String(review.reviewer));
            Ok(TomlValue::Table(table))
        })
        .collect()
}

fn verify_policy(safe_root: &Path) -> Result<VerificationContext> {
    let policy_path = safe_root.join("upstream-compat/safety-policy.toml");
    let fallback_path = safe_root.join("generated/baseline/fallback-c-inventory.json");
    let package_scope_path = safe_root.join("upstream-compat/package-scope.toml");
    let cve_status_path = safe_root.join("upstream-compat/cve-status.toml");
    let cve_index_path = safe_root.join("generated/security/relevant-cves-index.json");

    for path in [
        &policy_path,
        &fallback_path,
        &package_scope_path,
        &cve_status_path,
        &cve_index_path,
    ] {
        if !path.exists() {
            bail!("required policy input is missing: {}", path.display());
        }
    }

    let policy: TomlValue = toml::from_str(
        &fs::read_to_string(&policy_path)
            .with_context(|| format!("failed to read {}", policy_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", policy_path.display()))?;
    let fallback_json: JsonValue = serde_json::from_str(
        &fs::read_to_string(&fallback_path)
            .with_context(|| format!("failed to read {}", fallback_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", fallback_path.display()))?;
    let package_scope: TomlValue = toml::from_str(
        &fs::read_to_string(&package_scope_path)
            .with_context(|| format!("failed to read {}", package_scope_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", package_scope_path.display()))?;
    let cve_status: TomlValue = toml::from_str(
        &fs::read_to_string(&cve_status_path)
            .with_context(|| format!("failed to read {}", cve_status_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", cve_status_path.display()))?;
    let cve_index: JsonValue = serde_json::from_str(
        &fs::read_to_string(&cve_index_path)
            .with_context(|| format!("failed to read {}", cve_index_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", cve_index_path.display()))?;

    verify_policy_schema(&policy)?;
    verify_package_scope_cross_references(&policy, &fallback_json, &package_scope)?;
    enforce_cve_disposition(&policy, &cve_status, &cve_index)?;
    ensure_helper_status(&package_scope, "/usr/lib/pt_chown", "omitted_on_amd64")?;

    Ok(VerificationContext {
        policy,
        fallback_json,
        package_scope,
        cve_status,
        cve_index,
    })
}

fn verify_policy_schema(policy: &TomlValue) -> Result<()> {
    let phases = toml_string_array(policy.get("phases")).context("missing safety-policy phases")?;
    let expected_phases: BTreeSet<String> =
        PHASE_IDS.iter().map(|phase| phase.to_string()).collect();
    let actual_phases: BTreeSet<String> = phases.iter().cloned().collect();
    if actual_phases != expected_phases {
        bail!("safety-policy phase list does not match the workflow phase IDs");
    }

    let reviewed_unsafe = policy
        .get("reviewed_unsafe")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| anyhow!("missing reviewed_unsafe policy section"))?;
    let reviewed_fallback = policy
        .get("reviewed_fallback")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| anyhow!("missing reviewed_fallback policy section"))?;
    let policy_table = policy
        .get("policy")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| anyhow!("missing policy section"))?;
    let phase_modes = policy
        .get("phase_modes")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| anyhow!("missing phase_modes section"))?;

    require_required_fields(
        reviewed_unsafe,
        &[
            "site_id",
            "path",
            "line",
            "rationale",
            "reviewer",
            "owning_phase",
        ],
        "reviewed_unsafe",
    )?;
    require_required_fields(
        reviewed_fallback,
        &[
            "path",
            "classification",
            "owning_phase",
            "shipped",
            "audit_notes",
            "reviewer",
        ],
        "reviewed_fallback",
    )?;

    let deny_phase = policy_table
        .get("deny_shipped_temporary_fallback_binary_by_phase")
        .and_then(TomlValue::as_str)
        .ok_or_else(|| anyhow!("missing deny_shipped_temporary_fallback_binary_by_phase"))?;
    if deny_phase != "impl_10_final_fixup_and_audit" {
        bail!("unexpected deny phase for shipped temporary fallback binaries");
    }
    let deny_private_backend_phase = policy_table
        .get("deny_shipped_private_backend_dso_by_phase")
        .and_then(TomlValue::as_str)
        .ok_or_else(|| anyhow!("missing deny_shipped_private_backend_dso_by_phase"))?;
    if deny_private_backend_phase != "impl_10_final_fixup_and_audit" {
        bail!("unexpected deny phase for shipped private backend DSOs");
    }
    if !policy_table
        .get("forbid_shipped_private_backend_dsos")
        .and_then(TomlValue::as_bool)
        .unwrap_or(false)
    {
        bail!("safety policy must explicitly forbid shipped private backend DSOs");
    }

    for phase in PHASE_IDS {
        let mode = phase_modes
            .get(phase)
            .and_then(TomlValue::as_table)
            .ok_or_else(|| anyhow!("missing phase mode for {phase}"))?;
        let strongest = mode
            .get("strongest_mode")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("missing strongest_mode for {phase}"))?;
        if phase == "impl_01_safe_bootstrap" && strongest != "--verify-policy" {
            bail!("impl_01 strongest_mode must be --verify-policy");
        }
        if phase == "impl_10_final_fixup_and_audit"
            && !strongest.contains("--deny-shipped-private-backend-dsos")
        {
            bail!("impl_10 strongest_mode must include --deny-shipped-private-backend-dsos");
        }
    }

    Ok(())
}

fn verify_package_scope_cross_references(
    policy: &TomlValue,
    fallback_json: &JsonValue,
    package_scope: &TomlValue,
) -> Result<()> {
    let policy_table = policy
        .get("policy")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| anyhow!("missing policy section"))?;
    let package_paths = collect_package_scope_paths(package_scope)?;
    let fallback_entries = fallback_json
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("fallback inventory must contain an entries array"))?;
    if fallback_entries.is_empty() {
        bail!("fallback inventory must not be empty");
    }

    let mut fallback_by_path: BTreeMap<String, (String, bool, BTreeSet<String>)> = BTreeMap::new();
    for entry in fallback_entries {
        let path = json_string(entry.get("path")).context("fallback entry missing path")?;
        let classification = json_string(entry.get("classification"))
            .context("fallback entry missing classification")?;
        let owning_phase = json_string(entry.get("owning_phase"))
            .context("fallback entry missing owning_phase")?;
        let _audit_notes =
            json_string(entry.get("audit_notes")).context("fallback entry missing audit_notes")?;
        let shipped = entry
            .get("shipped")
            .and_then(JsonValue::as_bool)
            .ok_or_else(|| anyhow!("fallback entry missing shipped flag for {path}"))?;

        if !PHASE_IDS.contains(&owning_phase.as_str()) {
            bail!("fallback entry {path} references unknown phase {owning_phase}");
        }
        if classification == "temporary_fallback_binary"
            && !policy_table
                .get("forbid_shipped_temporary_fallback_binaries")
                .and_then(TomlValue::as_bool)
                .unwrap_or(false)
        {
            bail!("safety policy must explicitly forbid shipped temporary_fallback_binary entries");
        }
        if classification == "private_baseline_backend_dso"
            && shipped
            && policy_table
                .get("forbid_shipped_private_backend_dsos")
                .and_then(TomlValue::as_bool)
                .unwrap_or(false)
        {
            bail!("fallback inventory still ships private baseline backend DSO {path}");
        }

        let mut package_refs = BTreeSet::new();
        if let Some(refs) = entry
            .get("package_scope_refs")
            .and_then(JsonValue::as_array)
        {
            for reference in refs {
                let reference = json_string(Some(reference))
                    .context("fallback package_scope_refs must be string values")?;
                if !package_paths.contains(&reference) {
                    bail!(
                        "fallback entry {path} references missing package-scope path {reference}"
                    );
                }
                package_refs.insert(reference);
            }
        }

        fallback_by_path.insert(path, (classification, shipped, package_refs));
    }

    for (path, classification, shipped) in collect_required_package_fallback_assets(package_scope)?
    {
        let Some((actual_classification, actual_shipped, package_refs)) =
            fallback_by_path.get(&path)
        else {
            bail!("fallback inventory is missing required package asset {path} ({classification})");
        };
        if actual_classification != &classification {
            bail!(
                "fallback inventory classification mismatch for {path}: expected {classification}, found {actual_classification}"
            );
        }
        if *actual_shipped != shipped {
            bail!(
                "fallback inventory shipped flag mismatch for {path}: expected {shipped}, found {actual_shipped}"
            );
        }
        if !package_refs.contains(&path) {
            bail!("fallback inventory entry {path} must reference its package-scope path");
        }
    }

    Ok(())
}

fn enforce_reviewed_unsafe(safe_root: &Path, policy: &TomlValue) -> Result<()> {
    let expected = collect_unsafe_sites(safe_root)?
        .into_iter()
        .map(|site| (site.site_id.clone(), site))
        .collect::<BTreeMap<_, _>>();
    let actual = parse_reviewed_unsafe_entries(policy)?
        .into_iter()
        .map(|site| (site.site_id.clone(), site))
        .collect::<BTreeMap<_, _>>();

    if expected != actual {
        let missing = expected
            .keys()
            .filter(|site_id| !actual.contains_key(*site_id))
            .cloned()
            .collect::<Vec<_>>();
        let stale = actual
            .keys()
            .filter(|site_id| !expected.contains_key(*site_id))
            .cloned()
            .collect::<Vec<_>>();
        let mut failures = Vec::new();
        if !missing.is_empty() {
            failures.push(format!(
                "missing reviewed unsafe sites: {}",
                missing.join(", ")
            ));
        }
        if !stale.is_empty() {
            failures.push(format!("stale reviewed unsafe sites: {}", stale.join(", ")));
        }
        bail!(
            "audit-safety unsafe review mismatch:\n{}",
            failures.join("\n")
        );
    }

    Ok(())
}

fn enforce_reviewed_fallbacks(
    safe_root: &Path,
    policy: &TomlValue,
    fallback_json: &JsonValue,
) -> Result<()> {
    let expected = collect_reviewed_fallbacks(safe_root)?
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let actual = parse_reviewed_fallback_entries(policy)?
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();

    if expected != actual {
        let missing = expected
            .keys()
            .filter(|path| !actual.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>();
        let stale = actual
            .keys()
            .filter(|path| !expected.contains_key(*path))
            .cloned()
            .collect::<Vec<_>>();
        let mut failures = Vec::new();
        if !missing.is_empty() {
            failures.push(format!(
                "missing reviewed fallback entries: {}",
                missing.join(", ")
            ));
        }
        if !stale.is_empty() {
            failures.push(format!(
                "stale reviewed fallback entries: {}",
                stale.join(", ")
            ));
        }
        bail!(
            "audit-safety fallback review mismatch:\n{}",
            failures.join("\n")
        );
    }

    let inventory_paths = fallback_json
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("fallback inventory must contain an entries array"))?
        .iter()
        .filter_map(|entry| entry.get("path").and_then(JsonValue::as_str))
        .collect::<BTreeSet<_>>();
    for path in collect_compat_asm_paths(safe_root)? {
        if !inventory_paths.contains(path.as_str()) {
            bail!(
                "compat-asm path {} is not tracked in fallback-c-inventory.json",
                path
            );
        }
    }

    Ok(())
}

fn enforce_no_shipped_temporary_fallback_binaries(fallback_json: &JsonValue) -> Result<()> {
    let entries = fallback_json
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("fallback inventory must contain an entries array"))?;
    let shipped = entries
        .iter()
        .filter(|entry| {
            entry.get("classification").and_then(JsonValue::as_str)
                == Some("temporary_fallback_binary")
                && entry.get("shipped").and_then(JsonValue::as_bool) == Some(true)
        })
        .filter_map(|entry| entry.get("path").and_then(JsonValue::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !shipped.is_empty() {
        bail!(
            "temporary fallback binaries are still shipped:\n{}",
            shipped.join("\n")
        );
    }
    Ok(())
}

fn enforce_no_shipped_private_backend_dsos(
    package_scope: &TomlValue,
    fallback_json: &JsonValue,
) -> Result<()> {
    let mut failures = Vec::new();
    let files = package_scope
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("package-scope must contain files array"))?;
    for entry in files.iter().filter_map(TomlValue::as_table) {
        let path = entry
            .get("path")
            .and_then(TomlValue::as_str)
            .unwrap_or("<unknown>");
        let shipped = entry.get("shipped_status").and_then(TomlValue::as_str) == Some("shipped");
        if shipped
            && (entry.get("asset_kind").and_then(TomlValue::as_str)
                == Some("private_baseline_backend_dso")
                || path.starts_with("/usr/libexec/safelibs/backends/"))
        {
            failures.push(format!("package-scope: {path}"));
        }
    }

    let entries = fallback_json
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("fallback inventory must contain an entries array"))?;
    for entry in entries {
        let path = entry
            .get("path")
            .and_then(JsonValue::as_str)
            .unwrap_or("<unknown>");
        let shipped = entry.get("shipped").and_then(JsonValue::as_bool) == Some(true);
        if shipped
            && (entry.get("classification").and_then(JsonValue::as_str)
                == Some("private_baseline_backend_dso")
                || path.starts_with("/usr/libexec/safelibs/backends/"))
        {
            failures.push(format!("fallback inventory: {path}"));
        }
    }

    if !failures.is_empty() {
        bail!(
            "private baseline backend DSOs are still shipped:\n{}",
            failures.join("\n")
        );
    }
    Ok(())
}

fn enforce_cve_disposition(
    policy: &TomlValue,
    cve_status: &TomlValue,
    cve_index: &JsonValue,
) -> Result<()> {
    let policy_table = policy
        .get("policy")
        .and_then(TomlValue::as_table)
        .ok_or_else(|| anyhow!("missing policy section"))?;
    let cve_entries = cve_status
        .get("entries")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("cve-status must contain an entries array"))?;
    let allowed_cve_statuses: BTreeSet<String> = policy_table
        .get("allowed_cve_statuses")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("missing allowed_cve_statuses"))?
        .iter()
        .filter_map(TomlValue::as_str)
        .map(str::to_string)
        .collect();
    let indexed_cves: BTreeSet<String> = cve_index
        .get("cves")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("relevant-cves-index must contain a cves array"))?
        .iter()
        .filter_map(|entry| entry.get("id").and_then(JsonValue::as_str))
        .map(str::to_string)
        .collect();

    let mut status_ids = BTreeSet::new();
    for entry in cve_entries {
        let entry = entry
            .as_table()
            .ok_or_else(|| anyhow!("cve-status entry must be a table"))?;
        let id = entry
            .get("id")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("cve-status entry missing id"))?;
        let status = entry
            .get("status")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("cve-status entry missing status for {id}"))?;
        let rationale = entry
            .get("rationale")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("cve-status entry missing rationale for {id}"))?;
        if rationale.trim().is_empty() {
            bail!("cve-status rationale must not be empty for {id}");
        }
        if !allowed_cve_statuses.contains(status) {
            bail!("cve-status entry {id} uses unsupported status {status}");
        }
        status_ids.insert(id.to_string());
    }

    if status_ids != indexed_cves {
        bail!("cve-status entries do not match the generated relevant-cves index");
    }
    Ok(())
}

fn enforce_package_scope_clean(package_scope: &TomlValue) -> Result<()> {
    let files = package_scope
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("package-scope must contain files array"))?;
    let lingering = files
        .iter()
        .filter_map(TomlValue::as_table)
        .filter(|entry| {
            entry.get("shipped_status").and_then(TomlValue::as_str) == Some("shipped")
                && matches!(
                    entry.get("asset_kind").and_then(TomlValue::as_str),
                    Some("temporary_fallback_binary") | Some("private_baseline_backend_dso")
                )
        })
        .filter_map(|entry| entry.get("path").and_then(TomlValue::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !lingering.is_empty() {
        bail!(
            "package-scope still contains shipped temporary fallback binaries:\n{}",
            lingering.join("\n")
        );
    }
    Ok(())
}

pub(crate) fn collect_unsafe_sites(safe_root: &Path) -> Result<Vec<UnsafeSite>> {
    let regex =
        Regex::new(r"\bunsafe(\s+fn|\s+impl|\s+extern|\s*\{)").expect("unsafe regex must compile");
    let repo_root = safe_root
        .parent()
        .ok_or_else(|| anyhow!("safe root must live under the repo root"))?;
    let mut sites = Vec::new();
    for entry in WalkDir::new(safe_root.join("crates"))
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs")
        {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(repo_root)
            .with_context(|| format!("failed to strip repo prefix from {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for (index, line) in contents.lines().enumerate() {
            if regex.is_match(line) {
                sites.push(UnsafeSite {
                    site_id: format!("{rel}:{}", index + 1),
                    path: rel.clone(),
                    line: index + 1,
                    rationale: unsafe_rationale_for_path(&rel).to_string(),
                    reviewer: AUTO_REVIEWER.to_string(),
                    owning_phase: owner_phase_for_repo_path(&rel).to_string(),
                });
            }
        }
    }
    sites.sort();
    Ok(sites)
}

fn collect_reviewed_fallbacks(safe_root: &Path) -> Result<Vec<ReviewedFallback>> {
    let fallback_path = safe_root.join("generated/baseline/fallback-c-inventory.json");
    let fallback_json: JsonValue = serde_json::from_str(
        &fs::read_to_string(&fallback_path)
            .with_context(|| format!("failed to read {}", fallback_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", fallback_path.display()))?;
    let entries = fallback_json
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("fallback inventory must contain an entries array"))?;

    let mut reviews = Vec::with_capacity(entries.len());
    for entry in entries {
        reviews.push(ReviewedFallback {
            path: json_string(entry.get("path")).context("fallback entry missing path")?,
            classification: json_string(entry.get("classification"))
                .context("fallback entry missing classification")?,
            owning_phase: json_string(entry.get("owning_phase"))
                .context("fallback entry missing owning_phase")?,
            shipped: entry
                .get("shipped")
                .and_then(JsonValue::as_bool)
                .ok_or_else(|| anyhow!("fallback entry missing shipped flag"))?,
            audit_notes: json_string(entry.get("audit_notes"))
                .context("fallback entry missing audit_notes")?,
            reviewer: AUTO_REVIEWER.to_string(),
        });
    }
    reviews.sort();
    Ok(reviews)
}

fn collect_compat_asm_paths(safe_root: &Path) -> Result<Vec<String>> {
    let repo_root = safe_root
        .parent()
        .ok_or_else(|| anyhow!("safe root must live under the repo root"))?;
    let compat_root = safe_root.join("crates/compat-asm");
    if !compat_root.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in WalkDir::new(&compat_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_string_lossy() == "README.md" {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(repo_root)
            .with_context(|| {
                format!(
                    "failed to strip repo prefix from {}",
                    entry.path().display()
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        paths.push(rel);
    }
    paths.sort();
    Ok(paths)
}

fn parse_reviewed_unsafe_entries(policy: &TomlValue) -> Result<Vec<UnsafeSite>> {
    let entries = policy
        .get(REVIEWED_UNSAFE_ENTRIES_KEY)
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("missing {REVIEWED_UNSAFE_ENTRIES_KEY} array"))?;
    let mut sites = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry
            .as_table()
            .ok_or_else(|| anyhow!("reviewed unsafe entry must be a table"))?;
        sites.push(UnsafeSite {
            site_id: required_toml_string(entry, "site_id")?,
            path: required_toml_string(entry, "path")?,
            line: required_toml_integer(entry, "line")? as usize,
            rationale: required_toml_string(entry, "rationale")?,
            reviewer: required_toml_string(entry, "reviewer")?,
            owning_phase: required_toml_string(entry, "owning_phase")?,
        });
    }
    sites.sort();
    Ok(sites)
}

fn parse_reviewed_fallback_entries(policy: &TomlValue) -> Result<Vec<ReviewedFallback>> {
    let entries = policy
        .get(REVIEWED_FALLBACK_ENTRIES_KEY)
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("missing {REVIEWED_FALLBACK_ENTRIES_KEY} array"))?;
    let mut reviews = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry
            .as_table()
            .ok_or_else(|| anyhow!("reviewed fallback entry must be a table"))?;
        reviews.push(ReviewedFallback {
            path: required_toml_string(entry, "path")?,
            classification: required_toml_string(entry, "classification")?,
            owning_phase: required_toml_string(entry, "owning_phase")?,
            shipped: entry
                .get("shipped")
                .and_then(TomlValue::as_bool)
                .ok_or_else(|| anyhow!("reviewed fallback entry missing shipped"))?,
            audit_notes: required_toml_string(entry, "audit_notes")?,
            reviewer: required_toml_string(entry, "reviewer")?,
        });
    }
    reviews.sort();
    Ok(reviews)
}

fn unsafe_rationale_for_path(path: &str) -> &'static str {
    if path.contains("/allocator.rs") {
        "Allocator entrypoints require a narrow FFI boundary to the process heap."
    } else if path.contains("/entropy.rs") {
        "Entropy interfaces need direct kernel randomness access to preserve getrandom, getentropy, and arc4random semantics."
    } else if path.contains("/mmap.rs") {
        "mmap-family wrappers cross the raw libc boundary for virtual-memory management."
    } else if path.contains("/syscall.rs") || path.contains("/futex.rs") {
        "Linux syscall and futex wrappers require integerized raw kernel entrypoints."
    } else if path.contains("runtime_tools.rs") {
        "Runtime tool frontends use an exec boundary to hand off to tracked backend binaries."
    } else {
        "This unsafe site is a narrow FFI or process-boundary escape hatch reviewed for phase ownership."
    }
}

fn owner_phase_for_repo_path(path: &str) -> &'static str {
    match path {
        path if path.starts_with("safe/crates/ldso/") => "impl_04_loader_startup_secure_exec",
        path if path.starts_with("safe/crates/libc6/src/startup/") => {
            "impl_04_loader_startup_secure_exec"
        }
        path if path.starts_with("safe/crates/libc-support-tools/src/loader_tools.rs")
            || path.starts_with("safe/crates/libc-support-tools/src/bin/safe-loader-tool.rs") =>
        {
            "impl_04_loader_startup_secure_exec"
        }
        path if path.starts_with("safe/crates/network-identity/") => "impl_07_nss_resolver_nscd",
        path if path.starts_with("safe/crates/core-runtime/")
            || path.starts_with("safe/crates/libpthread/")
            || path.starts_with("safe/crates/libthread-db/")
            || path.starts_with("safe/crates/libc6/src/sys/")
            || path.starts_with("safe/crates/libc-support-tools/src/runtime_tools.rs")
            || path.starts_with("safe/crates/libc-support-tools/src/bin/safe-runtime-tool.rs") =>
        {
            "impl_05_core_runtime_threads_entropy"
        }
        _ => "impl_05_core_runtime_threads_entropy",
    }
}

fn required_toml_string(table: &toml::map::Map<String, TomlValue>, key: &str) -> Result<String> {
    table
        .get(key)
        .and_then(TomlValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("missing string field {key}"))
}

fn required_toml_integer(table: &toml::map::Map<String, TomlValue>, key: &str) -> Result<i64> {
    table
        .get(key)
        .and_then(TomlValue::as_integer)
        .ok_or_else(|| anyhow!("missing integer field {key}"))
}

fn require_required_fields(
    section: &toml::map::Map<String, TomlValue>,
    required: &[&str],
    section_name: &str,
) -> Result<()> {
    let fields = toml_string_array(section.get("required_fields"))
        .with_context(|| format!("missing {section_name}.required_fields"))?;
    let field_set: BTreeSet<String> = fields.into_iter().collect();
    for field in required {
        if !field_set.contains(*field) {
            bail!("{section_name}.required_fields is missing {field}");
        }
    }
    Ok(())
}

fn collect_package_scope_paths(package_scope: &TomlValue) -> Result<BTreeSet<String>> {
    let files = package_scope
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("package-scope must contain files array"))?;
    let mut paths = BTreeSet::new();
    for entry in files {
        let entry = entry
            .as_table()
            .ok_or_else(|| anyhow!("package-scope file entry must be a table"))?;
        let path = entry
            .get("path")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("package-scope file entry missing path"))?;
        let scope = entry
            .get("scope")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("package-scope file entry missing scope for {path}"))?;
        if !matches!(
            scope,
            "required_package" | "deferred_package" | "testroot_only"
        ) {
            bail!("package-scope file entry {path} has invalid scope {scope}");
        }
        paths.insert(path.to_string());
    }
    Ok(paths)
}

fn collect_required_package_fallback_assets(
    package_scope: &TomlValue,
) -> Result<Vec<(String, String, bool)>> {
    let files = package_scope
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("package-scope must contain files array"))?;
    let mut required = Vec::new();
    for entry in files {
        let entry = entry
            .as_table()
            .ok_or_else(|| anyhow!("package-scope file entry must be a table"))?;
        let path = entry
            .get("path")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("package-scope file entry missing path"))?;
        let scope = entry
            .get("scope")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("package-scope file entry missing scope for {path}"))?;
        let asset_kind = entry
            .get("asset_kind")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("package-scope file entry missing asset_kind for {path}"))?;
        if scope != "required_package"
            || !matches!(
                asset_kind,
                "asm_shim"
                    | "compat_asm"
                    | "temporary_fallback_binary"
                    | "tracked_backend_binary"
                    | "private_baseline_backend_dso"
            )
        {
            continue;
        }
        let shipped_status = entry
            .get("shipped_status")
            .and_then(TomlValue::as_str)
            .ok_or_else(|| anyhow!("package-scope file entry missing shipped_status for {path}"))?;
        required.push((
            path.to_string(),
            asset_kind.to_string(),
            shipped_status == "shipped",
        ));
    }
    Ok(required)
}

fn ensure_helper_status(package_scope: &TomlValue, path: &str, expected: &str) -> Result<()> {
    let helpers = package_scope
        .get("helper_paths")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("package-scope must contain helper_paths"))?;
    let helper = helpers
        .iter()
        .filter_map(TomlValue::as_table)
        .find(|entry| entry.get("path").and_then(TomlValue::as_str) == Some(path))
        .with_context(|| format!("package-scope helper_paths must include {path}"))?;
    let shipped_status = helper
        .get("shipped_status")
        .and_then(TomlValue::as_str)
        .ok_or_else(|| anyhow!("helper_paths entry missing shipped_status"))?;
    if shipped_status != expected {
        bail!("package-scope must record {path} as {expected}");
    }
    Ok(())
}

fn toml_string_array(value: Option<&TomlValue>) -> Result<Vec<String>> {
    let array = value
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow!("expected string array"))?;
    let mut values = Vec::with_capacity(array.len());
    for value in array {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("expected string array members"))?;
        values.push(value.to_string());
    }
    Ok(values)
}

fn json_string(value: Option<&JsonValue>) -> Result<String> {
    value
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("expected string value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_runtime_paths_to_phase_five() {
        assert_eq!(
            owner_phase_for_repo_path("safe/crates/core-runtime/src/entropy.rs"),
            "impl_05_core_runtime_threads_entropy"
        );
    }

    #[test]
    fn maps_network_identity_paths_to_phase_seven() {
        assert_eq!(
            owner_phase_for_repo_path("safe/crates/network-identity/src/lib.rs"),
            "impl_07_nss_resolver_nscd"
        );
    }

    #[test]
    fn selects_specific_unsafe_rationales() {
        assert!(
            unsafe_rationale_for_path("safe/crates/core-runtime/src/entropy.rs")
                .contains("getrandom")
        );
    }
}
