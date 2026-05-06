use crate::common::{load_toml, safe_root};
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use toml::Value as TomlValue;

const PHASE09_HELPERS: [&str; 5] = [
    "/usr/bin/gencat",
    "/usr/bin/getconf",
    "/usr/bin/tzselect",
    "/usr/bin/zdump",
    "/usr/sbin/zic",
];

const PHASE09_HELPER_BACKENDS: [&str; 5] = [
    "/usr/libexec/safelibs/aux-tools/gencat.backend",
    "/usr/libexec/safelibs/aux-tools/getconf.backend",
    "/usr/libexec/safelibs/aux-tools/tzselect.backend",
    "/usr/libexec/safelibs/aux-tools/zdump.backend",
    "/usr/libexec/safelibs/aux-tools/zic.backend",
];

#[derive(ClapArgs, Debug, Default)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    verify_phase09_helpers_are_not_fallback_wrappers()?;
    verify_phase09_helper_backends_removed()?;
    super::audit_safety::run(super::audit_safety::Args {
        verify_policy: true,
        deny_unreviewed_unsafe: true,
        deny_untracked_fallback_c: true,
        deny_shipped_temporary_fallback_binaries: false,
        require_cve_disposition: true,
        require_package_scope_clean: true,
    })
}

fn verify_phase09_helper_backends_removed() -> Result<()> {
    let package_scope = load_toml(&safe_root().join("upstream-compat/package-scope.toml"))?;
    let files = package_scope
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("package-scope files array is missing"))?;

    for backend in PHASE09_HELPER_BACKENDS {
        if files
            .iter()
            .filter_map(TomlValue::as_table)
            .any(|entry| entry.get("path").and_then(TomlValue::as_str) == Some(backend))
        {
            bail!("phase-09 helper backend {backend} is still recorded as shipped");
        }
    }
    Ok(())
}

fn verify_phase09_helpers_are_not_fallback_wrappers() -> Result<()> {
    let package_scope = load_toml(&safe_root().join("upstream-compat/package-scope.toml"))?;
    let files = package_scope
        .get("files")
        .and_then(TomlValue::as_array)
        .ok_or_else(|| anyhow::anyhow!("package-scope files array is missing"))?;

    for helper in PHASE09_HELPERS {
        let entry = files
            .iter()
            .filter_map(TomlValue::as_table)
            .find(|entry| entry.get("path").and_then(TomlValue::as_str) == Some(helper))
            .with_context(|| format!("package-scope is missing file entry {helper}"))?;
        let asset_kind = entry
            .get("asset_kind")
            .and_then(TomlValue::as_str)
            .with_context(|| format!("package-scope entry {helper} is missing asset_kind"))?;
        if asset_kind == "fallback_wrapper" {
            bail!("phase-09 helper {helper} still ships as a fallback wrapper");
        }
    }
    Ok(())
}
