use anyhow::{anyhow, bail, Result};
use std::env;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const RUNTIME_TOOL_BINARY_NAME: &str = "safe-runtime-tool";
pub const PLDD_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/runtime-tools/pldd.backend";

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| RUNTIME_TOOL_BINARY_NAME.to_string());
    let tool = Path::new(&argv0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(RUNTIME_TOOL_BINARY_NAME);

    match tool {
        "pldd" => run_pldd(&argv[1..]),
        other => bail!("unsupported runtime tool entrypoint {other}"),
    }
}

fn run_pldd(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help") {
        println!("Usage: pldd PID");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("pldd safelibs");
        return Ok(());
    }
    exec_backend(resolve_pldd_backend()?, args)
}

fn exec_backend(backend: PathBuf, args: &[String]) -> Result<()> {
    let mut command = Command::new(&backend);
    command.args(args);
    Err(command.exec().into())
}

fn resolve_pldd_backend() -> Result<PathBuf> {
    resolve_backend(
        PLDD_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/pldd"),
        Path::new("/usr/bin/pldd.real"),
    )
}

fn resolve_backend(installed: &str, repo: PathBuf, system: &Path) -> Result<PathBuf> {
    for candidate in [PathBuf::from(installed), repo, system.to_path_buf()] {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!("no backend payload is available for {installed}"))
}

fn repo_relative_backend(path: &str) -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|repo_root| repo_root.join(path))
        .unwrap_or_else(|| PathBuf::from(path))
}
