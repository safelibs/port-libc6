use anyhow::{anyhow, bail, Result};
use std::env;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const AUX_TOOL_BINARY_NAME: &str = "safe-aux-tool";
pub const AUX_TOOL_SOURCE_PATH: &str = "safe/crates/libc-support-tools/src/aux_tools.rs";
pub const GENCAT_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/aux-tools/gencat.backend";
pub const GETCONF_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/aux-tools/getconf.backend";
pub const TZSELECT_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/aux-tools/tzselect.backend";
pub const ZDUMP_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/aux-tools/zdump.backend";
pub const ZIC_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/aux-tools/zic.backend";

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| AUX_TOOL_BINARY_NAME.to_string());
    let tool = Path::new(&argv0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(AUX_TOOL_BINARY_NAME);

    match tool {
        "gencat" => exec_backend(resolve_gencat_backend()?, &argv[1..]),
        "getconf" => exec_backend(resolve_getconf_backend()?, &argv[1..]),
        "tzselect" => exec_backend(resolve_tzselect_backend()?, &argv[1..]),
        "zdump" => exec_backend(resolve_zdump_backend()?, &argv[1..]),
        "zic" => exec_backend(resolve_zic_backend()?, &argv[1..]),
        other => bail!("unsupported aux tool entrypoint {other}"),
    }
}

fn exec_backend(backend: PathBuf, args: &[String]) -> Result<()> {
    let mut command = Command::new(&backend);
    command.args(args);
    Err(command.exec().into())
}

fn resolve_gencat_backend() -> Result<PathBuf> {
    resolve_backend(
        GENCAT_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/gencat"),
    )
}

fn resolve_getconf_backend() -> Result<PathBuf> {
    resolve_backend(
        GETCONF_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/getconf"),
    )
}

fn resolve_tzselect_backend() -> Result<PathBuf> {
    resolve_backend(
        TZSELECT_BACKEND_INSTALL_PATH,
        repo_relative_backend("original/timezone/tzselect.ksh"),
    )
}

fn resolve_zdump_backend() -> Result<PathBuf> {
    resolve_backend(
        ZDUMP_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/zdump"),
    )
}

fn resolve_zic_backend() -> Result<PathBuf> {
    resolve_backend(
        ZIC_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/sbin/zic"),
    )
}

fn resolve_backend(installed: &str, repo: PathBuf) -> Result<PathBuf> {
    let mut candidates = vec![PathBuf::from(installed)];
    if let Some(path) = current_install_root_backend(installed) {
        candidates.push(path);
    }
    candidates.push(repo);
    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!("no backend payload is available for {installed}"))
}

fn current_install_root_backend(installed: &str) -> Option<PathBuf> {
    let relative = installed.strip_prefix("/usr/")?;
    let executable = env::current_exe().ok()?;
    let usr_root = executable.parent()?.parent()?;
    Some(usr_root.join(relative))
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
