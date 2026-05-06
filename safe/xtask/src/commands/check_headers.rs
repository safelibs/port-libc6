use crate::common::safe_root;
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use std::fs;
use std::path::{Path, PathBuf};
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
    #[arg(long, default_value_t = false)]
    pub all_installed: bool,
    #[arg(long, value_delimiter = ',')]
    pub feature_profiles: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let staged_build_root = super::stage_upstream_build::ensure_default_staged_upstream_build()?;
    let install_root = if args.install_root.is_absolute() {
        args.install_root
    } else {
        safe_root().join(args.install_root)
    };
    super::install_root::materialize_install_root(&install_root, false, true)?;
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
    let headers = if args.all_installed {
        installed_public_headers(&install_root)?
    } else {
        representative_headers(&install_root)?
    };
    let feature_profiles = resolve_feature_profiles(&args.feature_profiles)?;
    for lang in &langs {
        validate_lang(lang)?;
        for profile in &feature_profiles {
            compile_headers_for_profile(&install_root, &headers, lang, profile, &log_dir)?;
        }
        run_upstream_installed_header_script(&install_root, &headers, lang, &log_dir)?;
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

fn installed_public_headers(install_root: &Path) -> Result<Vec<String>> {
    let include_root = install_root.join("usr/include");
    if !include_root.is_dir() {
        bail!(
            "installed include root is missing: {}",
            include_root.display()
        );
    }
    let mut headers = Vec::new();
    for entry in walkdir::WalkDir::new(&include_root) {
        let entry = entry.with_context(|| format!("failed to walk {}", include_root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(&include_root)
            .with_context(|| format!("failed to strip prefix {}", include_root.display()))?;
        if !is_header_path(rel) {
            continue;
        }
        let rel = rel.to_string_lossy().replace('\\', "/");
        if should_skip_direct_header(&rel) {
            continue;
        }
        headers.push(rel);
    }
    headers.sort();
    headers.dedup();
    if headers.is_empty() {
        bail!(
            "no installed public headers were found under {}",
            include_root.display()
        );
    }
    Ok(headers)
}

fn is_header_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("h") | Some("hh") | Some("hpp") | Some("hxx")
    )
}

fn should_skip_direct_header(header: &str) -> bool {
    header.starts_with("bits/")
        || header.starts_with("finclude/")
        || (header.starts_with("gnu/lib-names-") && header.ends_with(".h"))
        || header == "regexp.h"
        || header == "sys/elf.h"
        || header == "sys/vm86.h"
}

#[derive(Clone, Copy, Debug)]
enum FeatureProfile {
    Default,
    Gnu,
    Posix,
    Xopen,
    LargeFile,
    Fortify,
}

impl FeatureProfile {
    fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Gnu => "gnu",
            Self::Posix => "posix",
            Self::Xopen => "xopen",
            Self::LargeFile => "large-file",
            Self::Fortify => "fortify",
        }
    }

    fn defines(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Default => &[],
            Self::Gnu => &[("_GNU_SOURCE", "1")],
            Self::Posix => &[("_POSIX_C_SOURCE", "200809L")],
            Self::Xopen => &[("_XOPEN_SOURCE", "700")],
            Self::LargeFile => &[("_FILE_OFFSET_BITS", "64")],
            Self::Fortify => &[("_FORTIFY_SOURCE", "3")],
        }
    }
}

fn resolve_feature_profiles(raw: &[String]) -> Result<Vec<FeatureProfile>> {
    let requested = if raw.is_empty() {
        vec![
            "default".to_string(),
            "gnu".to_string(),
            "posix".to_string(),
            "xopen".to_string(),
            "large-file".to_string(),
            "fortify".to_string(),
        ]
    } else {
        raw.to_vec()
    };
    let mut profiles = Vec::new();
    for profile in requested {
        let profile = match profile.as_str() {
            "default" => FeatureProfile::Default,
            "gnu" => FeatureProfile::Gnu,
            "posix" => FeatureProfile::Posix,
            "xopen" => FeatureProfile::Xopen,
            "large-file" => FeatureProfile::LargeFile,
            "fortify" => FeatureProfile::Fortify,
            other => bail!("unknown header feature profile {other}"),
        };
        profiles.push(profile);
    }
    Ok(profiles)
}

fn validate_lang(lang: &str) -> Result<()> {
    match lang {
        "c" | "c++" => Ok(()),
        other => bail!("unsupported header language {other}; expected c or c++"),
    }
}

fn compile_headers_for_profile(
    install_root: &Path,
    headers: &[String],
    lang: &str,
    profile: &FeatureProfile,
    log_dir: &Path,
) -> Result<()> {
    let compiler = if lang == "c++" { "g++" } else { "gcc" };
    let suffix = if lang == "c++" { "cc" } else { "c" };
    let standard = if lang == "c++" {
        "-std=c++17"
    } else {
        "-std=c11"
    };
    let source = log_dir.join(format!(
        "header-probe-{}-{}.{}",
        lang,
        profile.name(),
        suffix
    ));
    let mut failures = Vec::new();
    let mut log = String::new();

    for header in headers {
        let source_text = render_header_probe(header, profile);
        fs::write(&source, source_text)
            .with_context(|| format!("failed to write {}", source.display()))?;
        let mut command = Command::new(compiler);
        command
            .arg("-fsyntax-only")
            .arg("-finput-charset=ascii")
            .arg(standard)
            .arg("-isystem")
            .arg(install_root.join("usr/include"))
            .arg("-I")
            .arg(install_root.join("usr/include"))
            .arg("-D_ISOMAC");
        if matches!(profile, FeatureProfile::Fortify) {
            command.arg("-O2");
        }
        command.arg(&source);
        let debug = format!("{command:?}");
        let output = command
            .output()
            .with_context(|| format!("failed to spawn {debug}"))?;
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
        if !output.status.success() {
            failures.push(header.clone());
        }
    }

    let log_path = log_dir.join(format!("{}-{}-installed-headers.log", lang, profile.name()));
    fs::write(&log_path, log).with_context(|| format!("failed to write {}", log_path.display()))?;
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "{} {} installed-header probes failed for {} header(s); see {}",
            lang,
            profile.name(),
            failures.len(),
            log_path.display()
        )
    }
}

fn render_header_probe(header: &str, profile: &FeatureProfile) -> String {
    let mut text = String::new();
    text.push_str("#undef _LIBC\n");
    text.push_str("#undef _GNU_SOURCE\n");
    text.push_str("#undef _POSIX_C_SOURCE\n");
    text.push_str("#undef _XOPEN_SOURCE\n");
    text.push_str("#undef _FILE_OFFSET_BITS\n");
    text.push_str("#undef _FORTIFY_SOURCE\n");
    for (name, value) in profile.defines() {
        text.push_str(&format!("#define {name} {value}\n"));
    }
    text.push_str(&format!("#include <{header}>\n"));
    text.push_str("int safelibs_header_probe;\n");
    text
}

fn run_upstream_installed_header_script(
    install_root: &Path,
    headers: &[String],
    lang: &str,
    log_dir: &Path,
) -> Result<()> {
    let compiler = if lang == "c++" { "g++" } else { "gcc" };
    let compile_cmd = format!(
        "{compiler} -isystem {}/usr/include -I{}/usr/include -D_ISOMAC",
        install_root.display(),
        install_root.display()
    );
    let script = safe_root().join("tests/scripts/check-installed-headers.sh");
    let mut command = Command::new("sh");
    command.arg(&script).arg(lang).arg("3").arg(compile_cmd);
    for header in headers {
        command.arg(header);
    }
    run_logged_command(
        &mut command,
        &log_dir.join(format!("{lang}-upstream-installed-headers.log")),
    )
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
