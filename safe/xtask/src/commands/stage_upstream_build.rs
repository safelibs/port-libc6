use crate::common::{
    default_upstream_source_build_dir, repo_path, resolve_safe_workspace_path,
    resolve_upstream_source_build_dir, run_command, safe_root,
};
use anyhow::Result;
use clap::Args as ClapArgs;
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
    ensure_staged_upstream_build(Path::new("original"), &default_upstream_source_build_dir())
}

pub fn ensure_staged_upstream_build(source: &Path, build: &Path) -> Result<PathBuf> {
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        repo_path(source.display().to_string())
    };
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
