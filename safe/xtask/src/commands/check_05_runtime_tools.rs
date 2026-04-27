use crate::common::{load_json, load_toml, safe_root};
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

#[derive(ClapArgs, Debug, Default)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let package_scope = load_toml(&safe_root().join("upstream-compat/package-scope.toml"))?;
    let required_manifest: JsonValue =
        load_json(&safe_root().join("generated/install-manifests/required-packages.json"))?;
    let fallback_inventory: JsonValue =
        load_json(&safe_root().join("generated/baseline/fallback-c-inventory.json"))?;

    ensure_package_scope_asset_kind(&package_scope, "/usr/bin/pldd", "rust_target")?;
    ensure_package_scope_asset_kind(
        &package_scope,
        "/usr/libexec/safelibs/runtime-tools/pldd.backend",
        "tracked_backend_binary",
    )?;
    ensure_helper_status(&package_scope, "/usr/lib/pt_chown", "omitted_on_amd64")?;
    ensure_install_manifest_entry(&required_manifest, "/usr/bin/pldd")?;
    ensure_install_manifest_entry(
        &required_manifest,
        "/usr/libexec/safelibs/runtime-tools/pldd.backend",
    )?;
    ensure_fallback_inventory_entry(
        &fallback_inventory,
        "/usr/libexec/safelibs/runtime-tools/pldd.backend",
    )?;
    ensure_unshipped_fallback_entry(&fallback_inventory, "/usr/lib/pt_chown")?;

    Ok(())
}

fn ensure_package_scope_asset_kind(doc: &TomlValue, path: &str, expected: &str) -> Result<()> {
    let files = doc
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("package-scope files array is missing"))?;
    let entry = files
        .iter()
        .find(|entry| {
            entry
                .as_table()
                .and_then(|table| table.get("path"))
                .and_then(TomlValue::as_str)
                == Some(path)
        })
        .with_context(|| format!("package-scope is missing file entry {path}"))?;
    let actual = entry
        .as_table()
        .and_then(|table| table.get("asset_kind"))
        .and_then(TomlValue::as_str)
        .with_context(|| format!("package-scope entry {path} is missing asset_kind"))?;
    if actual != expected {
        bail!("package-scope asset_kind mismatch for {path}: expected {expected}, found {actual}");
    }
    Ok(())
}

fn ensure_helper_status(doc: &TomlValue, path: &str, expected: &str) -> Result<()> {
    let helpers = doc
        .get("helper_paths")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("package-scope helper_paths array is missing"))?;
    let entry = helpers
        .iter()
        .find(|entry| {
            entry
                .as_table()
                .and_then(|table| table.get("path"))
                .and_then(TomlValue::as_str)
                == Some(path)
        })
        .with_context(|| format!("package-scope is missing helper entry {path}"))?;
    let actual = entry
        .as_table()
        .and_then(|table| table.get("shipped_status"))
        .and_then(TomlValue::as_str)
        .with_context(|| format!("package-scope helper entry {path} is missing shipped_status"))?;
    if actual != expected {
        bail!(
            "package-scope shipped_status mismatch for {path}: expected {expected}, found {actual}"
        );
    }
    Ok(())
}

fn ensure_install_manifest_entry(doc: &JsonValue, path: &str) -> Result<()> {
    let entries = doc
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("install manifest entries array is missing"))?;
    if !entries
        .iter()
        .any(|entry| entry.get("path").and_then(JsonValue::as_str) == Some(path))
    {
        bail!("required install manifest is missing {path}");
    }
    Ok(())
}

fn ensure_fallback_inventory_entry(doc: &JsonValue, path: &str) -> Result<()> {
    let entries = doc
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("fallback inventory entries array is missing"))?;
    if !entries
        .iter()
        .any(|entry| entry.get("path").and_then(JsonValue::as_str) == Some(path))
    {
        bail!("fallback inventory is missing {path}");
    }
    Ok(())
}

fn ensure_unshipped_fallback_entry(doc: &JsonValue, path: &str) -> Result<()> {
    let entries = doc
        .get("entries")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("fallback inventory entries array is missing"))?;
    let entry = entries
        .iter()
        .find(|entry| entry.get("path").and_then(JsonValue::as_str) == Some(path))
        .with_context(|| format!("fallback inventory is missing {path}"))?;
    if entry.get("shipped").and_then(JsonValue::as_bool) != Some(false) {
        bail!("fallback inventory entry {path} must remain unshipped");
    }
    Ok(())
}
