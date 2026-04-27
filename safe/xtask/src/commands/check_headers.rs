use crate::common::safe_root;
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(
        long = "root",
        visible_alias = "install-root",
        default_value = "work/install-root"
    )]
    pub install_root: PathBuf,
    #[arg(long)]
    pub lang: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let staged_build_root = super::stage_upstream_build::ensure_default_staged_upstream_build()?;
    let install_root = if args.install_root.is_absolute() {
        args.install_root
    } else {
        safe_root().join(args.install_root)
    };
    super::install_root::materialize_install_root(&install_root, true, false)?;
    let log_dir = safe_root().join("work/check-headers");
    if log_dir.exists() {
        fs::remove_dir_all(&log_dir)
            .with_context(|| format!("failed to remove {}", log_dir.display()))?;
    }
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create {}", log_dir.display()))?;

    let langs = if args.lang.is_empty() {
        vec!["c".to_string(), "c++".to_string()]
    } else {
        args.lang
    };
    let headers = representative_headers(&install_root)?;
    let script = safe_root().join("tests/scripts/check-installed-headers.sh");
    for lang in langs {
        let compiler = if lang == "c++" { "g++" } else { "gcc" };
        let compile_cmd = format!(
            "{compiler} -isystem {}/usr/include -I{}/usr/include -D_ISOMAC",
            install_root.display(),
            install_root.display()
        );
        let mut command = Command::new("sh");
        command.arg(&script).arg(&lang).arg("3").arg(compile_cmd);
        for header in &headers {
            command.arg(header);
        }
        run_logged_command(
            &mut command,
            &log_dir.join(format!("{lang}-installed-headers.log")),
        )?;
    }

    // Reuse the upstream scripts directly for the phase-owned special checks too.
    run_logged_command(
        Command::new("bash")
            .env("AWK", "awk")
            .arg(safe_root().join("tests/scripts/check-local-headers.sh"))
            .arg(install_root.join("usr/include"))
            .arg(&staged_build_root),
        &log_dir.join("check-local-headers.log"),
    )?;
    let wrapper_headers = wrapper_headers()?;
    let tests_root = safe_root().join("tests");
    let mut wrapper_command = Command::new("python3");
    wrapper_command
        .current_dir(&tests_root)
        .arg(tests_root.join("scripts/check-wrapper-headers.py"))
        .arg("--root=.")
        .arg("--subdir=.")
        .args(wrapper_headers);
    run_logged_command(
        &mut wrapper_command,
        &log_dir.join("check-wrapper-headers.log"),
    )?;
    Ok(())
}

fn representative_headers(install_root: &PathBuf) -> Result<Vec<String>> {
    let include_root = install_root.join("usr/include");
    let mut headers = Vec::new();
    for header in [
        "stdio.h",
        "stdlib.h",
        "string.h",
        "unistd.h",
        "errno.h",
        "time.h",
        "dlfcn.h",
        "pthread.h",
        "setjmp.h",
        "malloc.h",
        "resolv.h",
        "netdb.h",
        "link.h",
        "locale.h",
        "dirent.h",
        "sys/types.h",
        "sys/stat.h",
        "sys/socket.h",
        "arpa/inet.h",
        "netinet/in.h",
    ] {
        if include_root.join(header).exists() {
            headers.push(header.to_string());
        }
    }
    if headers.is_empty() {
        anyhow::bail!(
            "no representative installed headers were found under {}",
            include_root.display()
        );
    }
    Ok(headers)
}

fn wrapper_headers() -> Result<Vec<String>> {
    let tests_root = safe_root().join("tests");
    let mut headers = Vec::new();
    for subdir in ["include", "bits"] {
        let root = tests_root.join(subdir);
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root) {
            let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(&root)
                .with_context(|| format!("failed to strip prefix {}", root.display()))?;
            let rel = rel.to_string_lossy().replace('\\', "/");
            if subdir == "bits" {
                headers.push(format!("bits/{rel}"));
            } else {
                headers.push(rel);
            }
        }
    }
    headers.sort();
    headers.dedup();
    if headers.is_empty() {
        bail!(
            "no wrapper headers were found under {}",
            tests_root.display()
        );
    }
    Ok(headers)
}

fn run_logged_command(command: &mut Command, log_path: &PathBuf) -> Result<()> {
    let debug = format!("{command:?}");
    let output = command
        .output()
        .with_context(|| format!("failed to spawn {debug}"))?;
    let mut log = String::new();
    log.push_str("$ ");
    log.push_str(&debug);
    log.push('\n');
    if !output.stdout.is_empty() {
        log.push_str("stdout:\n");
        log.push_str(&String::from_utf8_lossy(&output.stdout));
        if !log.ends_with('\n') {
            log.push('\n');
        }
    }
    if !output.stderr.is_empty() {
        log.push_str("stderr:\n");
        log.push_str(&String::from_utf8_lossy(&output.stderr));
        if !log.ends_with('\n') {
            log.push('\n');
        }
    }
    fs::write(log_path, log).with_context(|| format!("failed to write {}", log_path.display()))?;
    if !output.status.success() {
        bail!(
            "command failed ({}): {debug}; see {}",
            output.status,
            log_path.display()
        );
    }
    Ok(())
}
