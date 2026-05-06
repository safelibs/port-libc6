use crate::common::{
    abi_baselines, command_output, default_upstream_source_build_dir, safe_root, version_name_cmp,
    AbiBaseline,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub all_dsos: bool,
    #[arg(long)]
    pub dso: Vec<String>,
    #[arg(long)]
    pub build_root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub strict_symbol_metadata: bool,
}

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let build_root = resolve_build_root(args.build_root.clone())?;
    let selected = select_dsos(&args)?;
    for baseline in selected {
        check_one(&baseline, &build_root, args.strict_symbol_metadata)?;
    }
    Ok(())
}

fn select_dsos(args: &Args) -> Result<Vec<AbiBaseline>> {
    let baselines = abi_baselines()?;
    if args.all_dsos || args.dso.is_empty() {
        return Ok(baselines);
    }
    let wanted = resolve_requested_dso_ids(&args.dso, &baselines)?;
    let selected = baselines
        .into_iter()
        .filter(|baseline| wanted.contains(&baseline.dso_id))
        .collect::<Vec<_>>();
    if selected.len() != wanted.len() {
        bail!("requested DSO selection resolved ambiguously");
    }
    Ok(selected)
}

fn resolve_requested_dso_ids(
    requested: &[String],
    baselines: &[AbiBaseline],
) -> Result<BTreeSet<String>> {
    let aliases = dso_alias_map(baselines)?;
    let mut selected = BTreeSet::new();
    let mut unknown = Vec::new();
    for raw in requested {
        if let Some(dso_id) = aliases.get(raw) {
            selected.insert(dso_id.clone());
        } else {
            unknown.push(raw.clone());
        }
    }
    if !unknown.is_empty() {
        bail!("unknown DSO requested: {}", unknown.join(", "));
    }
    Ok(selected)
}

fn dso_alias_map(baselines: &[AbiBaseline]) -> Result<BTreeMap<String, String>> {
    let mut aliases = BTreeMap::new();
    for baseline in baselines {
        register_dso_alias(&mut aliases, &baseline.dso_id, &baseline.dso_id)?;
        if let Some(stripped) = baseline.dso_id.strip_suffix(".so") {
            register_dso_alias(&mut aliases, stripped, &baseline.dso_id)?;
        }
    }
    Ok(aliases)
}

fn register_dso_alias(
    aliases: &mut BTreeMap<String, String>,
    alias: &str,
    dso_id: &str,
) -> Result<()> {
    match aliases.get(alias) {
        Some(existing) if existing != dso_id => {
            bail!("conflicting ABI DSO alias {alias}: {existing} vs {dso_id}")
        }
        Some(_) => Ok(()),
        None => {
            aliases.insert(alias.to_string(), dso_id.to_string());
            Ok(())
        }
    }
}

fn resolve_build_root(build_root: Option<PathBuf>) -> Result<PathBuf> {
    match build_root {
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Ok(safe_root().join(path)),
        None => super::build::load_active_build_root().with_context(|| {
            "no active hybrid build root is recorded; run `cargo run -p xtask -- build --target amd64 --profile release` first"
        }),
    }
}

fn check_one(
    baseline: &AbiBaseline,
    build_root: &PathBuf,
    strict_symbol_metadata: bool,
) -> Result<()> {
    let artifact = resolve_artifact_path(baseline, build_root).with_context(|| {
        format!(
            "missing hybrid build artifact for {} under {}",
            baseline.dso_id,
            build_root.display()
        )
    })?;
    let dynamic = command_output(Command::new("readelf").arg("-d").arg(&artifact))?;
    if let Some(soname) = &baseline.soname {
        if !dynamic.contains(soname) {
            bail!(
                "{} has SONAME mismatch: expected {} in {}",
                baseline.dso_id,
                soname,
                artifact.display()
            );
        }
    }

    let version_info =
        command_output(Command::new("readelf").arg("--version-info").arg(&artifact))?;
    let dynsyms = command_output(
        Command::new("readelf")
            .arg("--dyn-syms")
            .arg("--wide")
            .arg(&artifact),
    )?;

    let version_script = safe_root()
        .join("generated/version-scripts")
        .join(format!("{}.map", baseline.dso_id));
    let generated = std::fs::read_to_string(&version_script)
        .with_context(|| format!("failed to read {}", version_script.display()))?;
    let mut version_names = baseline
        .map_files
        .iter()
        .filter(|map| map.kind == "version_script")
        .flat_map(|map| map.versions.keys().cloned())
        .collect::<Vec<_>>();
    version_names.sort_by(|left, right| version_name_cmp(left, right));
    version_names.dedup();
    for version in version_names {
        if !generated.contains(&version) {
            bail!(
                "generated map {} is missing version {}",
                version_script.display(),
                version
            );
        }
        if !version_info.contains(&version) {
            bail!(
                "runtime artifact {} is missing version {}",
                artifact.display(),
                version
            );
        }
    }

    let exported = parse_defined_dynsyms(&dynsyms);
    for symbol in expected_exported_symbols(baseline) {
        if !exported.contains(&symbol) && !has_compatible_export(&exported, &symbol) {
            bail!(
                "runtime artifact {} is missing exported symbol {}",
                artifact.display(),
                symbol
            );
        }
    }
    if strict_symbol_metadata {
        check_strict_symbol_metadata(baseline, &artifact, &dynamic, &version_info, &dynsyms)?;
    }
    Ok(())
}

fn check_strict_symbol_metadata(
    baseline: &AbiBaseline,
    artifact: &PathBuf,
    dynamic: &str,
    version_info: &str,
    dynsyms: &str,
) -> Result<()> {
    let oracle = resolve_original_oracle_path(baseline)?;
    let oracle_dynamic = command_output(Command::new("readelf").arg("-d").arg(&oracle))?;
    let oracle_version_info =
        command_output(Command::new("readelf").arg("--version-info").arg(&oracle))?;
    let oracle_dynsyms = command_output(
        Command::new("readelf")
            .arg("--dyn-syms")
            .arg("--wide")
            .arg(&oracle),
    )?;

    let mut failures = Vec::new();
    let actual_soname = parse_soname(dynamic);
    if actual_soname != baseline.soname {
        failures.push(format!(
            "SONAME mismatch for {}: expected {:?}, found {:?}",
            baseline.dso_id, baseline.soname, actual_soname
        ));
    }
    let actual_needed = parse_needed(dynamic);
    let expected_needed = baseline.needed.iter().cloned().collect::<BTreeSet<_>>();
    if actual_needed != expected_needed
        && !allowed_synthetic_needed_delta(baseline, &actual_needed, &expected_needed)
    {
        failures.push(format!(
            "DT_NEEDED mismatch for {}: expected {:?}, found {:?}",
            baseline.dso_id, expected_needed, actual_needed
        ));
    }
    let oracle_needed = parse_needed(&oracle_dynamic);
    if actual_needed != oracle_needed
        && !allowed_synthetic_needed_delta(baseline, &actual_needed, &oracle_needed)
    {
        failures.push(format!(
            "DT_NEEDED differs from original oracle for {}: original {:?}, safe {:?}",
            baseline.dso_id, oracle_needed, actual_needed
        ));
    }

    let actual_version_defs = parse_version_definition_names(version_info);
    let oracle_version_defs = parse_version_definition_names(&oracle_version_info);
    if actual_version_defs != oracle_version_defs {
        failures.push(format!(
            "version definitions differ for {}: original {:?}, safe {:?}",
            baseline.dso_id, oracle_version_defs, actual_version_defs
        ));
    }

    let actual_symbols = parse_symbol_metadata(dynsyms);
    let oracle_symbols = parse_symbol_metadata(&oracle_dynsyms);
    for (key, expected) in &oracle_symbols {
        let Some(actual) = actual_symbols.get(key) else {
            failures.push(format!(
                "{} is missing strict symbol {} from {}",
                artifact.display(),
                key,
                oracle.display()
            ));
            continue;
        };
        if actual.binding != expected.binding {
            failures.push(format!(
                "{} binding mismatch: expected {}, found {}",
                key, expected.binding, actual.binding
            ));
        }
        if actual.symbol_type != expected.symbol_type {
            failures.push(format!(
                "{} type mismatch: expected {}, found {}",
                key, expected.symbol_type, actual.symbol_type
            ));
        }
        if actual.visibility != expected.visibility {
            failures.push(format!(
                "{} visibility mismatch: expected {}, found {}",
                key, expected.visibility, actual.visibility
            ));
        }
        if expected.size != 0
            && actual.size != expected.size
            && !is_synthetic_version_placeholder_key(key)
        {
            failures.push(format!(
                "{} size mismatch: expected {}, found {}",
                key, expected.size, actual.size
            ));
        }
        if matches!(expected.symbol_type.as_str(), "OBJECT" | "TLS" | "IFUNC")
            && actual.symbol_type != expected.symbol_type
        {
            failures.push(format!(
                "{} exported data/TLS/IFUNC class mismatch: expected {}, found {}",
                key, expected.symbol_type, actual.symbol_type
            ));
        }
    }
    for key in actual_symbols.keys() {
        if !oracle_symbols.contains_key(key) {
            failures.push(format!(
                "{} exports unexpected strict symbol {}",
                artifact.display(),
                key
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "strict ABI metadata failures for {}:\n{}",
            baseline.dso_id,
            failures.join("\n")
        )
    }
}

fn resolve_artifact_path(baseline: &AbiBaseline, build_root: &PathBuf) -> Result<PathBuf> {
    for path in &baseline.installed_paths {
        let rel = path.trim_start_matches('/');
        let staged = build_root.join(rel);
        if is_elf(&staged) {
            return Ok(staged);
        }
        if let Some(rest) = path.strip_prefix("/usr/lib64/") {
            let alt = build_root.join("lib64").join(rest);
            if is_elf(&alt) {
                return Ok(alt);
            }
        }
    }
    Err(anyhow!(
        "none of the installed paths exist in the hybrid build root for {}",
        baseline.dso_id
    ))
}

fn resolve_original_oracle_path(baseline: &AbiBaseline) -> Result<PathBuf> {
    let original_build_root = default_upstream_source_build_dir();
    let mut candidates = vec![original_build_root.join(&baseline.primary_oracle)];
    if let Some(stripped) = baseline.primary_oracle.strip_prefix("build/") {
        candidates.push(original_build_root.join(stripped));
    }
    for path in &baseline.installed_paths {
        candidates.push(
            original_build_root
                .join("testroot.pristine")
                .join(path.trim_start_matches('/')),
        );
    }
    for candidate in candidates {
        if is_elf(&candidate) {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "no original ABI oracle exists for {} under {}",
        baseline.dso_id,
        original_build_root.display()
    ))
}

fn is_elf(path: &PathBuf) -> bool {
    if !path.exists() {
        return false;
    }
    Command::new("readelf")
        .arg("-h")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_soname(dynamic: &str) -> Option<String> {
    dynamic.lines().find_map(|line| {
        let start = line.find("Library soname: [")? + "Library soname: [".len();
        let end = line[start..].find(']')? + start;
        Some(line[start..end].to_string())
    })
}

fn parse_needed(dynamic: &str) -> BTreeSet<String> {
    dynamic
        .lines()
        .filter_map(|line| {
            let start = line.find("Shared library: [")? + "Shared library: [".len();
            let end = line[start..].find(']')? + start;
            Some(line[start..end].to_string())
        })
        .collect()
}

fn parse_version_definition_names(version_info: &str) -> BTreeSet<String> {
    let mut in_definitions = false;
    let mut names = BTreeSet::new();
    for line in version_info.lines() {
        if line.contains("Version definition section") {
            in_definitions = true;
            continue;
        }
        if line.contains("Version needs section") {
            in_definitions = false;
        }
        if !in_definitions {
            continue;
        }
        for part in line.split("Name:").skip(1) {
            let name = part.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    names
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SymbolMetadata {
    binding: String,
    symbol_type: String,
    visibility: String,
    size: u64,
}

fn parse_symbol_metadata(dynsyms: &str) -> BTreeMap<String, SymbolMetadata> {
    let mut symbols = BTreeMap::new();
    for line in dynsyms.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 8 || !fields[0].ends_with(':') || fields[6] == "UND" {
            continue;
        }
        if fields[4] == "LOCAL" {
            continue;
        }
        let Some(key) = symbol_metadata_key(fields[7]) else {
            continue;
        };
        let size = fields[2].parse::<u64>().unwrap_or(0);
        symbols.insert(
            key,
            SymbolMetadata {
                binding: fields[4].to_string(),
                symbol_type: fields[3].to_string(),
                visibility: fields[5].to_string(),
                size,
            },
        );
    }
    symbols
}

fn symbol_metadata_key(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let rendered = if let Some((name, version)) = raw.split_once("@@") {
        if is_synthetic_version_placeholder(name) {
            format!("{name}@{version}")
        } else {
            format!("{name}@@{version}")
        }
    } else if let Some((name, version)) = raw.split_once('@') {
        format!("{name}@{version}")
    } else {
        raw.to_string()
    };
    Some(rendered)
}

fn is_synthetic_version_placeholder(name: &str) -> bool {
    name.ends_with("_version_placeholder")
}

fn is_synthetic_version_placeholder_key(key: &str) -> bool {
    is_synthetic_version_placeholder(key.split('@').next().unwrap_or(key))
}

fn allowed_synthetic_needed_delta(
    baseline: &AbiBaseline,
    actual: &BTreeSet<String>,
    expected: &BTreeSet<String>,
) -> bool {
    // libanl is a no_std version-placeholder shim in the safe tree.  It has no
    // libc relocations, so the linker legitimately emits no DT_NEEDED entry.
    baseline.dso_id == "libanl"
        && actual.is_empty()
        && expected.len() == 1
        && expected.contains("libc.so.6")
}

fn parse_defined_dynsyms(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 8 || !fields[0].ends_with(':') {
            continue;
        }
        if fields[6] == "UND" {
            continue;
        }
        names.insert(fields[7].to_string());
    }
    names
}

fn has_compatible_export(exported: &BTreeSet<String>, expected: &str) -> bool {
    let Some((base, version)) = split_symbol_version(expected) else {
        return exported.contains(expected);
    };
    exported.iter().any(|candidate| {
        if candidate == base {
            return true;
        }
        let Some((candidate_base, candidate_version)) = split_symbol_version(candidate) else {
            return candidate == base;
        };
        candidate_base == base && version_matches(version, candidate_version)
    })
}

fn split_symbol_version(symbol: &str) -> Option<(&str, &str)> {
    if let Some((base, version)) = symbol.split_once("@@") {
        return Some((base, version));
    }
    symbol.split_once('@')
}

fn version_matches(expected: &str, candidate: &str) -> bool {
    candidate == expected || candidate.starts_with(&format!("{expected}."))
}

fn expected_exported_symbols(baseline: &AbiBaseline) -> impl Iterator<Item = String> + '_ {
    baseline
        .exported_symbols
        .iter()
        .filter(|raw| !is_export_marker(raw))
        .cloned()
}

fn is_export_marker(raw: &str) -> bool {
    raw == "Name" || raw.starts_with("GLIBC_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_baseline(dso_id: &str) -> AbiBaseline {
        AbiBaseline {
            dso_id: dso_id.to_string(),
            primary_oracle: String::new(),
            auxiliary_oracles: Vec::new(),
            installed_paths: Vec::new(),
            build_id: None,
            soname: None,
            needed: Vec::new(),
            exported_symbols: Vec::new(),
            map_files: Vec::new(),
            symlist_files: Vec::new(),
        }
    }

    #[test]
    fn resolves_ld_alias_to_loader_baseline() {
        let baselines = vec![sample_baseline("ld.so"), sample_baseline("libc")];
        let resolved =
            resolve_requested_dso_ids(&["ld".to_string(), "libc".to_string()], &baselines)
                .expect("aliases should resolve");
        assert_eq!(
            resolved,
            BTreeSet::from(["ld.so".to_string(), "libc".to_string()])
        );
    }

    #[test]
    fn rejects_unknown_dso_alias() {
        let baselines = vec![sample_baseline("ld.so")];
        let error = resolve_requested_dso_ids(&["libpthread".to_string()], &baselines)
            .expect_err("unknown alias should fail");
        assert!(error.to_string().contains("unknown DSO requested"));
    }
}
