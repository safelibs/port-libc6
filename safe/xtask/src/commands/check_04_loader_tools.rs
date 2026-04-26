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

    for path in ["/usr/bin/ld.so", "/usr/bin/ldd", "/usr/sbin/ldconfig"] {
        ensure_package_scope_asset_kind(&package_scope, path, "rust_target")?;
    }
    for path in [
        "/usr/libexec/safelibs/loader-tools/ld.so.backend",
        "/usr/libexec/safelibs/loader-tools/ldconfig.backend",
    ] {
        ensure_package_scope_asset_kind(&package_scope, path, "temporary_fallback_binary")?;
        ensure_install_manifest_entry(&required_manifest, path)?;
        ensure_fallback_inventory_entry(&fallback_inventory, path)?;
    }

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
