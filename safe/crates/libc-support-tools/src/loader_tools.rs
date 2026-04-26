use anyhow::{anyhow, bail, Context, Result};
use ldso::{
    current_process_auxv, default_tunable_registry, secure_exec_env, secure_exec_env_from_pairs,
    LoaderInvocation, LoaderMode, TunableKind,
};
use std::env;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

pub const LOADER_TOOL_BINARY_NAME: &str = "safe-loader-tool";
pub const LDSO_BACKEND_INSTALL_PATH: &str = "/usr/libexec/safelibs/loader-tools/ld.so.backend";
pub const LDCONFIG_BACKEND_INSTALL_PATH: &str =
    "/usr/libexec/safelibs/loader-tools/ldconfig.backend";

pub fn main_from_env() -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "safe-loader-tool".to_string());
    let tool = Path::new(&argv0)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("safe-loader-tool");

    match tool {
        "ldd" => run_ldd(&argv[1..]),
        "ld.so" | "ld-linux-x86-64.so.2" => run_ldso(&argv[1..]),
        "ldconfig" => run_ldconfig(&argv[1..]),
        other => bail!("unsupported loader tool entrypoint {other}"),
    }
}

fn run_ldd(args: &[String]) -> Result<()> {
    let mut warn = false;
    let mut bind_now = false;
    let mut verbose = false;
    let mut unused = false;
    let mut files = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--version" | "--vers" | "--versi" | "--versio" => {
                println!("ldd safelibs");
                return Ok(());
            }
            "--help" | "--h" | "--he" | "--hel" => {
                println!(
                    "Usage: ldd [OPTION]... FILE...\n  --help\n  --version\n  -d, --data-relocs\n  -r, --function-relocs\n  -u, --unused\n  -v, --verbose"
                );
                return Ok(());
            }
            "-d" | "--data-relocs" => warn = true,
            "-r" | "--function-relocs" => {
                warn = true;
                bind_now = true;
            }
            "-u" | "--unused" => unused = true,
            "-v" | "--verbose" => verbose = true,
            "--" => {}
            other if other.starts_with('-') => bail!("unsupported ldd option {other}"),
            file => files.push(file.to_string()),
        }
    }

    if files.is_empty() {
        bail!("ldd: missing file arguments");
    }

    let rtld = resolve_ldso_backend()?;
    let mut had_error = false;
    let single = files.len() == 1;

    for file in files {
        if !single {
            println!("{file}:");
        }
        let path = if file.contains('/') {
            PathBuf::from(&file)
        } else {
            PathBuf::from(format!("./{file}"))
        };
        if !path.exists() {
            eprintln!("ldd: {}: No such file or directory", path.display());
            had_error = true;
            continue;
        }
        if !path.is_file() {
            eprintln!("ldd: {}: not regular file", path.display());
            had_error = true;
            continue;
        }

        let verify = Command::new(&rtld)
            .arg("--verify")
            .arg(&path)
            .status()
            .with_context(|| format!("failed to verify {}", path.display()))?;
        match verify.code() {
            Some(0) | Some(2) => {}
            Some(1) => {
                eprintln!("\tnot a dynamic executable");
                had_error = true;
                continue;
            }
            _ => bail!(
                "ldd: {} exited with unexpected status {verify}",
                rtld.display()
            ),
        }

        let mut command = Command::new(&rtld);
        command.arg(&path);
        command.env("LD_TRACE_LOADED_OBJECTS", "1");
        command.env("LD_WARN", if warn { "1" } else { "" });
        command.env("LD_BIND_NOW", if bind_now { "1" } else { "" });
        command.env("LD_VERBOSE", if verbose { "1" } else { "" });
        if unused {
            let prior = env::var("LD_DEBUG").ok();
            let updated = match prior {
                Some(existing) if !existing.is_empty() => format!("{existing},unused"),
                _ => "unused".to_string(),
            };
            command.env("LD_DEBUG", updated);
        }
        let output = command
            .output()
            .with_context(|| format!("failed to trace {}", path.display()))?;
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        if !output.status.success() {
            had_error = true;
        }
    }

    if had_error {
        bail!("ldd failed");
    }
    Ok(())
}

fn run_ldso(args: &[String]) -> Result<()> {
    let invocation = LoaderInvocation::parse(args);
    match invocation.mode.clone() {
        Some(LoaderMode::Help) => {
            println!(
                "Usage: ld.so [OPTION]... EXECUTABLE [ARGS...]\n  --list-tunables\n  --verify FILE\n  --library-path PATH\n  --glibc-hwcaps-prepend LIST\n  --glibc-hwcaps-mask LIST"
            );
            Ok(())
        }
        Some(LoaderMode::Version) => {
            println!("ld.so safelibs");
            Ok(())
        }
        Some(LoaderMode::ListTunables) => {
            for definition in default_tunable_registry().definitions() {
                match definition.kind {
                    TunableKind::String => println!("{}: string", definition.name),
                    TunableKind::Int32 { min, max } => {
                        println!("{}: int [{}..{}]", definition.name, min, max)
                    }
                    TunableKind::Uint64 { min, max } => {
                        println!("{}: uint [{}..{}]", definition.name, min, max)
                    }
                }
            }
            Ok(())
        }
        Some(LoaderMode::Verify { .. }) | Some(LoaderMode::Execute { .. }) | None => {
            exec_backend(resolve_ldso_backend()?, args)
        }
    }
}

fn run_ldconfig(args: &[String]) -> Result<()> {
    if args.is_empty()
        && env::var_os("LDCONFIG_NOTRIGGER").is_none()
        && env::var_os("DPKG_MAINTSCRIPT_PACKAGE").is_some()
        && run_status(Command::new("dpkg-trigger").arg("--check-supported"))?.success()
        && run_status(
            Command::new("dpkg-trigger")
                .arg("--no-await")
                .arg("ldconfig"),
        )?
        .success()
    {
        return Ok(());
    }

    exec_backend(resolve_ldconfig_backend()?, args)
}

fn exec_backend(backend: PathBuf, args: &[String]) -> Result<()> {
    let secure = current_process_auxv()
        .map(|auxv| auxv.secure())
        .unwrap_or(false);
    let mut command = Command::new(&backend);
    command.args(args);
    if secure {
        command.env_clear();
        for (key, value) in secure_exec_env() {
            command.env(key, value);
        }
    } else if let Ok(auxv) = current_process_auxv() {
        if auxv.secure() {
            command.env_clear();
            for (key, value) in secure_exec_env_from_pairs(env::vars()) {
                command.env(key, value);
            }
        }
    }
    Err(command.exec().into())
}

fn run_status(command: &mut Command) -> Result<ExitStatus> {
    command
        .status()
        .with_context(|| format!("failed to run {:?}", command))
}

fn resolve_ldso_backend() -> Result<PathBuf> {
    resolve_backend(
        LDSO_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/lib64/ld-linux-x86-64.so.2"),
        Path::new("/lib64/ld-linux-x86-64.so.2"),
    )
}

fn resolve_ldconfig_backend() -> Result<PathBuf> {
    resolve_backend(
        LDCONFIG_BACKEND_INSTALL_PATH,
        repo_relative_backend("build/testroot.pristine/usr/sbin/ldconfig"),
        Path::new("/sbin/ldconfig.real"),
    )
}

fn resolve_backend(installed: &str, repo: PathBuf, system: &Path) -> Result<PathBuf> {
    for candidate in [PathBuf::from(installed), repo, system.to_path_buf()] {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!("no backend payload is available for {}", installed))
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
