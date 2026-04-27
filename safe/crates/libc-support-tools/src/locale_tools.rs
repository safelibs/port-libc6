use anyhow::{anyhow, bail, Result};
use std::env;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const LOCALE_TOOL_BINARY_NAME: &str = "safe-locale-tool";
pub const LOCALE_TOOL_SOURCE_PATH: &str = "safe/crates/libc-support-tools/src/locale_tools.rs";
pub const ICONV_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/locale-tools/iconv.backend";
pub const ICONVCONFIG_BACKEND_INSTALL_PATH: &str =
    "/usr/libexec/safelibs/locale-tools/iconvconfig.backend";
pub const LOCALE_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/locale-tools/locale.backend";
pub const LOCALEDEF_BACKEND_INSTALL_PATH: &str =
    "/usr/libexec/safelibs/locale-tools/localedef.backend";

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| LOCALE_TOOL_BINARY_NAME.to_string());
    let tool = Path::new(&argv0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(LOCALE_TOOL_BINARY_NAME);

    match tool {
        "iconv" => exec_backend(resolve_iconv_backend()?, &argv[1..]),
        "iconvconfig" => exec_backend(resolve_iconvconfig_backend()?, &argv[1..]),
        "locale" => exec_backend(resolve_locale_backend()?, &argv[1..]),
        "localedef" => exec_backend(resolve_localedef_backend()?, &argv[1..]),
        other => bail!("unsupported locale tool entrypoint {other}"),
    }
}

fn exec_backend(backend: PathBuf, args: &[String]) -> Result<()> {
    let mut command = Command::new(&backend);
    command.args(args);
    Err(command.exec().into())
}

fn resolve_iconv_backend() -> Result<PathBuf> {
    resolve_backend(
        ICONV_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/iconv"),
    )
}

fn resolve_iconvconfig_backend() -> Result<PathBuf> {
    resolve_backend(
        ICONVCONFIG_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/sbin/iconvconfig"),
    )
}

fn resolve_locale_backend() -> Result<PathBuf> {
    resolve_backend(
        LOCALE_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/locale"),
    )
}

fn resolve_localedef_backend() -> Result<PathBuf> {
    resolve_backend(
        LOCALEDEF_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/bin/localedef"),
    )
}

fn resolve_backend(installed: &str, repo: PathBuf) -> Result<PathBuf> {
    for candidate in [PathBuf::from(installed), repo] {
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
