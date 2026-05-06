use crate::common::{
    command_output, install_path_to_root, link_compat_corpus_path, load_link_compat_corpus,
    make_ld_library_path, repo_path, resolve_safe_workspace_path,
    resolve_upstream_source_build_dir, run_command, safe_root,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(
        long = "install-root",
        visible_alias = "root",
        default_value = "work/install-root"
    )]
    pub install_root: PathBuf,
    #[arg(long, default_value = "work/original-build")]
    pub build_root: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    if !link_compat_corpus_path().exists() {
        bail!(
            "missing committed relink oracle {}; phase 06 requires this corpus",
            link_compat_corpus_path().display()
        );
    }

    super::build::run(super::build::Args {
        target: "amd64".to_string(),
        profile: "dev".to_string(),
    })?;
    super::stage_upstream_build::ensure_staged_upstream_build(
        Path::new("original"),
        &args.build_root,
    )?;

    let install_root = resolve_safe_workspace_path(&args.install_root)?;
    let build_root = resolve_upstream_source_build_dir(&args.build_root)?;
    super::install_root::materialize_install_root(&install_root, true, false)?;

    let corpus = load_link_compat_corpus()?;
    let scratch = safe_root().join("work/link-smoke");
    let original_objects_root = scratch.join("original-objects");
    let relink_root = scratch.join("relinked");
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .with_context(|| format!("failed to remove {}", scratch.display()))?;
    }
    fs::create_dir_all(&original_objects_root)
        .with_context(|| format!("failed to create {}", original_objects_root.display()))?;
    fs::create_dir_all(&relink_root)
        .with_context(|| format!("failed to create {}", relink_root.display()))?;

    let original_sysroot = build_root.join("testroot.pristine");
    for case in corpus.cases {
        let original_object = materialize_original_object(
            &case,
            &original_sysroot,
            &build_root,
            &original_objects_root,
        )?;
        let binary = relink_case(
            &case,
            &original_object,
            &original_sysroot,
            &install_root,
            &relink_root,
        )?;
        run_case(&case, &binary, &install_root)?;
    }
    Ok(())
}

fn materialize_original_object(
    case: &crate::common::LinkCompatCase,
    original_sysroot: &Path,
    build_root: &Path,
    original_objects_root: &Path,
) -> Result<PathBuf> {
    let output = original_objects_root.join(&case.original_object_relpath);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    match case.object_source_kind.as_str() {
        "original_sysroot_fixture" => {
            let source = case
                .fixture_source_path
                .as_deref()
                .ok_or_else(|| anyhow!("{} is missing fixture_source_path", case.case_id))?;
            let source = repo_path(source);
            let mut command = Command::new("gcc");
            command
                .arg(format!("--sysroot={}", original_sysroot.display()))
                .arg("-I")
                .arg(original_sysroot.join("usr/include"))
                .arg("-c")
                .arg(&source)
                .arg("-o")
                .arg(&output);
            for arg in &case.compile_args {
                command.arg(arg);
            }
            run_command(&mut command).with_context(|| {
                format!(
                    "failed to compile original fixture {} for {}",
                    source.display(),
                    case.case_id
                )
            })?;
        }
        "harvested_upstream_object" => {
            let source = case
                .upstream_object_path
                .as_deref()
                .ok_or_else(|| anyhow!("{} is missing upstream_object_path", case.case_id))?;
            let source = build_root.join(source);
            crate::common::copy_file_or_symlink(&source, &output)?;
        }
        other => bail!("unsupported link-compat object_source_kind {other}"),
    }
    Ok(output)
}

fn relink_case(
    case: &crate::common::LinkCompatCase,
    original_object: &Path,
    original_sysroot: &Path,
    install_root: &Path,
    relink_root: &Path,
) -> Result<PathBuf> {
    let binary = relink_root.join(&case.case_id);
    if let Some(parent) = binary.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let libdir = install_root.join("usr/lib64");
    let mut command = Command::new("gcc");
    command
        .arg(format!("--sysroot={}", install_root.display()))
        .arg("-B")
        .arg(&libdir)
        .arg("-L")
        .arg(&libdir)
        .arg("-Wl,-rpath-link")
        .arg(&libdir)
        .arg("-Wl,-dynamic-linker,/usr/lib64/ld-linux-x86-64.so.2");
    if !case_requests_pie(case) {
        command.arg("-no-pie");
    }
    for startfile in &case.required_startfiles {
        command.arg(original_sysroot.join(startfile.trim_start_matches('/')));
    }
    command.arg("-o").arg(&binary).arg(original_object);
    for arg in &case.link_args {
        command.arg(arg);
    }
    run_command(&mut command)
        .with_context(|| format!("failed to relink compatibility case {}", case.case_id))?;
    Ok(binary)
}

fn case_requests_pie(case: &crate::common::LinkCompatCase) -> bool {
    case.coverage_class == "pie"
        || case.coverage_class == "static-pie"
        || case
            .link_args
            .iter()
            .any(|arg| arg == "-pie" || arg == "-static-pie")
}

fn run_case(
    case: &crate::common::LinkCompatCase,
    binary: &Path,
    install_root: &Path,
) -> Result<()> {
    match case.run_mode.as_str() {
        "skip" => Ok(()),
        "direct" => run_direct_case(case, binary),
        "safe-loader" => {
            let backend_root = install_path_to_root(install_root, "/usr/libexec/safelibs/backends");
            let library_path = format!(
                "{}:{}",
                backend_root.display(),
                make_ld_library_path(install_root)
            );
            let output = command_output(
                Command::new(install_path_to_root(
                    install_root,
                    "/usr/lib64/ld-linux-x86-64.so.2",
                ))
                .env("SAFELIBS_BACKEND_ROOT", &backend_root)
                .arg("--library-path")
                .arg(library_path)
                .arg(binary),
            )?;
            ensure_no_runtime_failure(case, &output)
        }
        other => bail!("unsupported link-compat run_mode {other}"),
    }
}

fn run_direct_case(case: &crate::common::LinkCompatCase, binary: &Path) -> Result<()> {
    let debug = format!("{:?}", Command::new(binary));
    let output = Command::new(binary)
        .output()
        .with_context(|| format!("failed to spawn {debug}"))?;
    if output.status.success() {
        return ensure_no_runtime_failure(case, &String::from_utf8_lossy(&output.stdout));
    }
    if case.coverage_class == "static-pie" {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("command failed ({}): {debug}\n{stderr}", output.status);
}

fn ensure_no_runtime_failure(case: &crate::common::LinkCompatCase, output: &str) -> Result<()> {
    if output.contains("error:") {
        bail!("link-compat case {} emitted failure text", case.case_id);
    }
    Ok(())
}
