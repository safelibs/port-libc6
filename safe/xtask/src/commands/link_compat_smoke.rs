use crate::common::{
    command_output, install_path_to_root, link_compat_corpus_path, load_link_compat_corpus,
    make_ld_library_path, repo_path, resolve_safe_workspace_path,
    resolve_upstream_source_build_dir, run_command, safe_root,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
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

    super::build::ensure_active_build_profile("amd64", "release")?;
    super::stage_upstream_build::ensure_staged_upstream_build(
        Path::new("original"),
        &args.build_root,
    )?;

    let install_root = resolve_safe_workspace_path(&args.install_root)?;
    let build_root = resolve_upstream_source_build_dir(&args.build_root)?;
    super::install_root::materialize_install_root(&install_root, false, true)?;

    let corpus = load_link_compat_corpus()?;
    verify_final_corpus_coverage(&corpus)?;
    verify_final_manifest_closure()?;
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
        verify_relinked_needed(&case, &binary)?;
        run_case(&case, &binary, &install_root)?;
    }
    Ok(())
}

fn verify_final_corpus_coverage(corpus: &crate::common::LinkCompatCorpus) -> Result<()> {
    let coverage_classes = corpus
        .cases
        .iter()
        .map(|case| case.coverage_class.clone())
        .collect::<BTreeSet<_>>();
    let startfiles = corpus
        .cases
        .iter()
        .flat_map(|case| case.required_startfiles.iter().cloned())
        .collect::<BTreeSet<_>>();

    let required_coverage = [
        "ordinary-dynamic",
        "pie",
        "static",
        "static-pie",
        "startup-object",
        "glibc-private",
        "profiling-startfile",
        "profiling-startfile-static-pie",
    ];
    let required_startfiles = [
        "/usr/lib64/Mcrt1.o",
        "/usr/lib64/Scrt1.o",
        "/usr/lib64/crt1.o",
        "/usr/lib64/crti.o",
        "/usr/lib64/crtn.o",
        "/usr/lib64/gcrt1.o",
        "/usr/lib64/grcrt1.o",
        "/usr/lib64/rcrt1.o",
    ];

    let mut failures = Vec::new();
    for coverage in required_coverage {
        if !coverage_classes.contains(coverage) {
            failures.push(format!(
                "committed link-compat corpus is missing {coverage} coverage"
            ));
        }
    }
    for startfile in required_startfiles {
        if !startfiles.contains(startfile) {
            failures.push(format!(
                "committed link-compat corpus never references required startfile {startfile}"
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "final link-compat corpus coverage is incomplete:\n{}",
            failures.join("\n")
        )
    }
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

fn verify_final_manifest_closure() -> Result<()> {
    let mut failures = Vec::new();
    for dir in [
        safe_root().join("generated/baseline/package-files"),
        safe_root().join("generated/install-manifests"),
    ] {
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry?;
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let doc: JsonValue = serde_json::from_str(
                &fs::read_to_string(entry.path())
                    .with_context(|| format!("failed to read {}", entry.path().display()))?,
            )
            .with_context(|| format!("failed to parse {}", entry.path().display()))?;
            for item in doc
                .get("entries")
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
            {
                let path = item
                    .get("path")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("<unknown>");
                let asset_kind = item
                    .get("asset_kind")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default();
                if asset_kind == "private_baseline_backend_dso"
                    || path.starts_with("/usr/libexec/safelibs/backends/")
                {
                    failures.push(format!(
                        "{} contains private backend {}",
                        entry.path().display(),
                        path
                    ));
                }
                if item.get("package").and_then(JsonValue::as_str) == Some("libc6-dev")
                    && item.get("source_origin").and_then(JsonValue::as_str)
                        == Some("build_testroot")
                    && is_code_bearing_dev_link_path(path)
                {
                    failures.push(format!(
                        "{} keeps code-bearing libc6-dev payload {} on build_testroot",
                        entry.path().display(),
                        path
                    ));
                }
            }
        }
    }
    let package_scope = fs::read_to_string(safe_root().join("upstream-compat/package-scope.toml"))
        .context("failed to read package-scope.toml")?;
    if package_scope.contains("asset_kind = \"private_baseline_backend_dso\"")
        || package_scope.contains("path = \"/usr/libexec/safelibs/backends/")
    {
        failures.push("package-scope still references a private backend DSO".to_string());
    }
    if !failures.is_empty() {
        bail!(
            "final link-compat manifest closure failed:\n{}",
            failures.join("\n")
        );
    }
    Ok(())
}

fn is_code_bearing_dev_link_path(path: &str) -> bool {
    (path.starts_with("/usr/lib64/") || path.starts_with("/usr/lib64/audit/"))
        && (path.ends_with(".o") || path.ends_with(".a") || path.ends_with(".so"))
}

fn relink_case(
    case: &crate::common::LinkCompatCase,
    original_object: &Path,
    _original_sysroot: &Path,
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
        command.arg(install_root.join(startfile.trim_start_matches('/')));
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
            let safe_library_path = make_ld_library_path(install_root);
            let mut command = Command::new(install_path_to_root(
                install_root,
                "/usr/lib64/ld-linux-x86-64.so.2",
            ));
            command
                .env("LD_DEBUG", "libs")
                .arg("--library-path")
                .arg(safe_library_path)
                .arg(binary);
            let debug = format!("{command:?}");
            let output = command
                .output()
                .with_context(|| format!("failed to spawn {debug}"))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{stdout}{stderr}");
            if !output.status.success() {
                bail!("command failed ({}): {debug}\n{combined}", output.status);
            }
            ensure_no_runtime_failure(case, &combined)?;
            if combined.contains("/usr/libexec/safelibs/backends") {
                bail!(
                    "link-compat case {} resolved through a private backend payload",
                    case.case_id
                );
            }
            verify_public_runtime_linkage(case, install_root, &combined)
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

fn verify_relinked_needed(case: &crate::common::LinkCompatCase, binary: &Path) -> Result<()> {
    let expected = expected_dt_needed_sonames(case);
    if expected.is_empty() {
        return Ok(());
    }

    let dynamic =
        command_output(Command::new("readelf").arg("-d").arg(binary)).with_context(|| {
            format!(
                "failed to inspect dynamic dependencies for link-compat case {}",
                case.case_id
            )
        })?;
    let actual = parse_needed_sonames(&dynamic);
    let missing = expected
        .iter()
        .copied()
        .filter(|soname| !actual.contains(*soname))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "link-compat case {} relinked binary {} is missing DT_NEEDED for {}",
            case.case_id,
            binary.display(),
            missing.join(", ")
        );
    }
    Ok(())
}

fn parse_needed_sonames(dynamic: &str) -> BTreeSet<String> {
    dynamic
        .lines()
        .filter_map(|line| {
            let start = line.find("Shared library: [")? + "Shared library: [".len();
            let end = line[start..].find(']')? + start;
            Some(line[start..end].to_string())
        })
        .collect()
}

fn expected_dt_needed_sonames(case: &crate::common::LinkCompatCase) -> Vec<&'static str> {
    case.exercised_dsos
        .iter()
        .filter_map(|dso| match dso.as_str() {
            "libanl" => Some("libanl.so.1"),
            "libm" => Some("libm.so.6"),
            "libresolv" => Some("libresolv.so.2"),
            _ => None,
        })
        .collect()
}

fn verify_public_runtime_linkage(
    case: &crate::common::LinkCompatCase,
    install_root: &Path,
    loader_output: &str,
) -> Result<()> {
    let expected = expected_phase07_public_sonames(case);
    if expected.is_empty() {
        return Ok(());
    }

    let missing = expected
        .iter()
        .filter_map(|(dso, soname)| {
            let path = install_path_to_root(install_root, &format!("/usr/lib64/{soname}"));
            let rendered = path.display().to_string();
            if loader_output.contains(&rendered) {
                None
            } else {
                Some(format!("{dso} ({rendered})"))
            }
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "link-compat case {} did not load public install-root DSO(s): {}",
            case.case_id,
            missing.join(", ")
        );
    }
    Ok(())
}

fn expected_phase07_public_sonames(
    case: &crate::common::LinkCompatCase,
) -> Vec<(&str, &'static str)> {
    if case.owner_phase != "impl_07_nss_resolver_nscd" {
        return Vec::new();
    }
    case.exercised_dsos
        .iter()
        .filter_map(|dso| {
            let soname = match dso.as_str() {
                "libanl" => "libanl.so.1",
                "libresolv" => "libresolv.so.2",
                "libnss_compat" => "libnss_compat.so.2",
                "libnss_dns" => "libnss_dns.so.2",
                "libnss_files" => "libnss_files.so.2",
                "libnss_hesiod" => "libnss_hesiod.so.2",
                _ => return None,
            };
            Some((dso.as_str(), soname))
        })
        .collect()
}
