use crate::common::{install_path_to_root, run_command, safe_root};
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PHASE_06_PUBLIC_DSO_PATHS: [(&str, &str); 6] = [
    ("ld.so", "/usr/lib64/ld-linux-x86-64.so.2"),
    ("libc", "/usr/lib64/libc.so.6"),
    ("libpthread", "/usr/lib64/libpthread.so.0"),
    ("libthread_db", "/usr/lib64/libthread_db.so.1"),
    ("libc_malloc_debug", "/usr/lib64/libc_malloc_debug.so.0"),
    ("libmemusage", "/usr/lib64/libmemusage.so"),
];

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub build_root: Option<PathBuf>,
    #[arg(
        long = "install-root",
        visible_alias = "root",
        default_value = "work/install-root"
    )]
    pub install_root: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    super::build::run(super::build::Args {
        target: "amd64".to_string(),
        profile: "dev".to_string(),
    })?;
    super::check_abi::run(super::check_abi::Args {
        all_dsos: false,
        dso: vec![
            "ld.so".to_string(),
            "libc".to_string(),
            "libpthread".to_string(),
            "libthread_db".to_string(),
            "libc_malloc_debug".to_string(),
            "libmemusage".to_string(),
        ],
        build_root: args.build_root.clone(),
        strict_symbol_metadata: false,
    })?;
    ensure_phase06_public_artifacts_are_safe_built(args.build_root.clone())?;
    super::link_compat_smoke::run(super::link_compat_smoke::Args {
        install_root: args.install_root.clone(),
        build_root: args
            .build_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("work/original-build")),
        strict_dev_assets: false,
    })?;
    super::check_headers::run(super::check_headers::Args {
        install_root: args.install_root,
        lang: Vec::new(),
        all_installed: false,
        feature_profiles: Vec::new(),
    })
}

fn ensure_phase06_public_artifacts_are_safe_built(build_root: Option<PathBuf>) -> Result<()> {
    let build_root = resolve_build_root(build_root)?;
    let original_root = safe_root().join("work/original-build/testroot.pristine");
    let scratch = safe_root().join("work/abi-public-cutover-compare");
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .with_context(|| format!("failed to remove {}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch)
        .with_context(|| format!("failed to create {}", scratch.display()))?;

    for (dso_id, install_path) in PHASE_06_PUBLIC_DSO_PATHS {
        let artifact = install_path_to_root(&build_root, install_path);
        let baseline = resolve_original_payload(&original_root, install_path)?;
        let stripped = scratch.join(format!("{dso_id}.without-safelibs-note"));
        fs::copy(&artifact, &stripped).with_context(|| {
            format!(
                "failed to prepare comparison copy {} from {}",
                stripped.display(),
                artifact.display()
            )
        })?;
        run_command(
            Command::new("objcopy")
                .arg("--remove-section")
                .arg(".note.safelibs")
                .arg(&stripped),
        )?;
        let generated = fs::read(&stripped)
            .with_context(|| format!("failed to read {}", stripped.display()))?;
        let baseline_bytes = fs::read(&baseline)
            .with_context(|| format!("failed to read {}", baseline.display()))?;
        if generated == baseline_bytes {
            bail!(
                "phase-06 public artifact {} at {} is still the baseline upstream DSO after removing .note.safelibs",
                dso_id,
                artifact.display()
            );
        }
    }
    Ok(())
}

fn resolve_build_root(build_root: Option<PathBuf>) -> Result<PathBuf> {
    match build_root {
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Ok(safe_root().join(path)),
        None => super::build::load_active_build_root(),
    }
}

fn resolve_original_payload(original_root: &Path, install_path: &str) -> Result<PathBuf> {
    let direct = original_root.join(install_path.trim_start_matches('/'));
    if direct.exists() {
        return Ok(direct);
    }
    if let Some(rest) = install_path.strip_prefix("/usr/lib64/") {
        let alt = original_root.join("lib64").join(rest);
        if alt.exists() {
            return Ok(alt);
        }
    }
    bail!(
        "missing original payload {} under {}",
        install_path,
        original_root.display()
    )
}
