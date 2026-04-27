use crate::common::{
    default_upstream_source_build_dir, repo_path, resolve_safe_workspace_path,
    resolve_upstream_source_build_dir, run_command, safe_root,
};
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "original")]
    pub source: PathBuf,
    #[arg(long, default_value = "work/original-build")]
    pub build: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    ensure_staged_upstream_build(&args.source, &args.build).map(|_| ())
}

pub fn ensure_default_staged_upstream_build() -> Result<PathBuf> {
    ensure_staged_upstream_build(&repo_path("original"), &default_upstream_source_build_dir())
}

pub fn ensure_staged_upstream_build(source: &Path, build: &Path) -> Result<PathBuf> {
    let source = resolve_source_path(source)?;
    let build = resolve_safe_workspace_path(build)?;
    let script = safe_root().join("scripts/stage-original-build.sh");
    run_command(
        Command::new("bash")
            .arg(&script)
            .arg("--source")
            .arg(&source)
            .arg("--build")
            .arg(&build),
    )?;
    resolve_upstream_source_build_dir(&build)
}

fn resolve_source_path(source: &Path) -> Result<PathBuf> {
    if source.is_absolute() {
        return Ok(fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf()));
    }

    let current_dir = std::env::current_dir().context("failed to determine current directory")?;
    let from_current_dir = current_dir.join(source);
    if from_current_dir.exists() {
        return Ok(fs::canonicalize(&from_current_dir).unwrap_or(from_current_dir));
    }

    let from_repo_root = repo_path(source.display().to_string());
    if from_repo_root.exists() {
        return Ok(fs::canonicalize(&from_repo_root).unwrap_or(from_repo_root));
    }

    Ok(from_current_dir)
}
