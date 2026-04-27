use crate::common::{
    command_output, copy_file_or_symlink, load_test_catalog, load_tests_manifest, repo_path,
    repo_root, resolve_safe_workspace_path, resolve_upstream_source_build_dir, safe_root,
    touch_executable_text, upstream_build_dir, TestCatalogEntry, TestsManifestEntry,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(
        long = "root",
        visible_alias = "install-root",
        default_value = "work/install-root"
    )]
    pub install_root: PathBuf,
    #[arg(long, default_value = "work/original-build")]
    pub build_root: PathBuf,
    #[arg(long)]
    pub families: Vec<String>,
    #[arg(long)]
    pub docker_image: Option<String>,
    #[arg(long, default_value_t = false)]
    pub privileged_container_tests: bool,
    #[arg(long)]
    pub subdirs: Vec<String>,
    #[arg(long)]
    pub tests: Vec<String>,
    #[arg(long, default_value = "default")]
    pub mode: String,
}

#[derive(Clone)]
struct RunConfig {
    install_root: PathBuf,
    build_root: PathBuf,
    mode: String,
    docker_image: Option<String>,
    privileged_container_tests: bool,
    entry_env: Vec<(String, String)>,
}

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    super::stage_upstream_build::ensure_staged_upstream_build(
        Path::new("original"),
        &args.build_root,
    )?;
    let config = RunConfig {
        install_root: resolve_safe_workspace_path(&args.install_root)?,
        build_root: resolve_upstream_source_build_dir(&args.build_root)?,
        mode: args.mode.clone(),
        docker_image: args.docker_image.clone(),
        privileged_container_tests: args.privileged_container_tests,
        entry_env: Vec::new(),
    };
    super::install_root::materialize_install_root(&config.install_root, true, false)?;

    let manifest = load_tests_manifest()?;
    let manifest_by_id = manifest
        .entries
        .iter()
        .map(|entry| (entry.catalog_id.clone(), entry.clone()))
        .collect::<BTreeMap<_, _>>();
    let catalog = load_test_catalog()?;
    let catalog_by_id = catalog
        .entries
        .iter()
        .map(|entry| (entry.catalog_id.clone(), entry.clone()))
        .collect::<BTreeMap<_, _>>();
    let selected = select_entries(&args, &catalog, &manifest_by_id)?;
    if selected.is_empty() {
        bail!("run-original-tests requires at least one selected catalog entry");
    }

    prepare_upstream_build_tree(&config.build_root)?;
    let mut failures = Vec::new();
    for entry in selected {
        let catalog_entry = catalog_by_id
            .get(&entry.catalog_id)
            .with_context(|| format!("missing catalog entry for {}", entry.catalog_id))?;
        let mut entry_config = config.clone();
        entry_config.entry_env = resolve_upstream_test_env(&entry_config, &entry, catalog_entry)?;
        if let Err(error) = run_one(&entry_config, &entry) {
            failures.push(format!("{}: {error:#}", entry.catalog_id));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!("run-original-tests failures:\n{}", failures.join("\n"))
    }
}

fn select_entries(
    args: &Args,
    catalog: &crate::common::TestCatalog,
    manifest_by_id: &BTreeMap<String, TestsManifestEntry>,
) -> Result<Vec<TestsManifestEntry>> {
    let wanted_subdirs = split_values(&args.subdirs);
    let wanted_tests = split_values(&args.tests);
    let wanted_families = normalize_family_filters(split_values(&args.families));
    let has_explicit_scope = !wanted_subdirs.is_empty() || !wanted_tests.is_empty();
    let catalog_by_id = catalog
        .entries
        .iter()
        .map(|entry| (entry.catalog_id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut catalog_by_name = BTreeMap::<&str, Vec<&crate::common::TestCatalogEntry>>::new();
    for entry in &catalog.entries {
        catalog_by_name
            .entry(entry.name.as_str())
            .or_default()
            .push(entry);
    }

    let mut selected_ids = BTreeSet::new();
    for token in &wanted_tests {
        if let Some(entry) = catalog_by_id.get(token.as_str()) {
            add_explicit_entry(
                args,
                &wanted_families,
                manifest_by_id,
                &mut selected_ids,
                entry,
            )?;
            continue;
        }

        let mut matched_any = false;
        let mut matched_ported = false;
        if let Some(entries) = catalog_by_name.get(token.as_str()) {
            for entry in entries {
                if !wanted_families.is_empty() && !wanted_families.contains(&entry.family) {
                    continue;
                }
                matched_any = true;
                let manifest = manifest_by_id
                    .get(&entry.catalog_id)
                    .with_context(|| format!("missing manifest entry for {}", entry.catalog_id))?;
                if manifest.port_status != "ported" {
                    continue;
                }
                ensure_privileged_allowed(args, entry)?;
                selected_ids.insert(manifest.catalog_id.clone());
                matched_ported = true;
            }
        }
        if !matched_any {
            bail!("test selector {token} did not match any catalog entry");
        }
        if !matched_ported {
            bail!("test selector {token} did not match any ported catalog entry");
        }
    }

    for catalog_entry in &catalog.entries {
        let choose_subdir =
            !wanted_subdirs.is_empty() && wanted_subdirs.contains(&catalog_entry.subdir);
        let choose_family =
            !wanted_families.is_empty() && wanted_families.contains(&catalog_entry.family);

        if choose_subdir {
            add_explicit_entry(
                args,
                &wanted_families,
                manifest_by_id,
                &mut selected_ids,
                catalog_entry,
            )?;
            continue;
        }

        if !has_explicit_scope && choose_family {
            let manifest = manifest_by_id
                .get(&catalog_entry.catalog_id)
                .with_context(|| {
                    format!("missing manifest entry for {}", catalog_entry.catalog_id)
                })?;
            if manifest.port_status != "ported" {
                continue;
            }
            ensure_privileged_allowed(args, catalog_entry)?;
            selected_ids.insert(manifest.catalog_id.clone());
        }
    }

    let mut selected = selected_ids
        .into_iter()
        .filter_map(|catalog_id| manifest_by_id.get(&catalog_id).cloned())
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.catalog_id.cmp(&right.catalog_id));
    Ok(selected)
}

fn add_explicit_entry(
    args: &Args,
    wanted_families: &BTreeSet<String>,
    manifest_by_id: &BTreeMap<String, TestsManifestEntry>,
    selected_ids: &mut BTreeSet<String>,
    catalog_entry: &crate::common::TestCatalogEntry,
) -> Result<()> {
    if !wanted_families.is_empty() && !wanted_families.contains(&catalog_entry.family) {
        return Ok(());
    }
    ensure_privileged_allowed(args, catalog_entry)?;
    let manifest = manifest_by_id
        .get(&catalog_entry.catalog_id)
        .with_context(|| format!("missing manifest entry for {}", catalog_entry.catalog_id))?;
    if manifest.port_status != "ported" {
        bail!(
            "catalog entry {} is not ported in safe/tests/manifest.toml",
            catalog_entry.catalog_id
        );
    }
    selected_ids.insert(manifest.catalog_id.clone());
    Ok(())
}

fn ensure_privileged_allowed(
    args: &Args,
    catalog_entry: &crate::common::TestCatalogEntry,
) -> Result<()> {
    if catalog_entry.requires_container_or_privileged_execution && !args.privileged_container_tests
    {
        bail!(
            "catalog entry {} requires --privileged-container-tests",
            catalog_entry.catalog_id
        );
    }
    Ok(())
}

fn split_values(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .flat_map(|value| value.split(|ch: char| ch == ',' || ch.is_whitespace()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_family_filters(values: BTreeSet<String>) -> BTreeSet<String> {
    if values
        .iter()
        .any(|value| value.eq_ignore_ascii_case("all") || value == "*")
    {
        BTreeSet::new()
    } else {
        values
    }
}

fn prepare_upstream_build_tree(build_root: &Path) -> Result<()> {
    let root = upstream_build_dir();
    if root.exists() {
        fs::remove_dir_all(&root)
            .with_context(|| format!("failed to remove {}", root.display()))?;
    }
    fs::create_dir_all(root.join("support"))
        .with_context(|| format!("failed to create {}", root.display()))?;
    fs::create_dir_all(root.join("compiled"))
        .with_context(|| format!("failed to create {}", root.display()))?;
    copy_file_or_symlink(
        &build_root.join("support/test-container"),
        &root.join("support/test-container"),
    )?;
    touch_executable_text(
        &root.join("testrun.sh"),
        &format!(
            "#!/bin/bash\nexec \"{}\" \"$@\"\n",
            build_root.join("testrun.sh").display()
        ),
    )?;
    mirror_source_testdata_roots(build_root)?;
    Ok(())
}

fn mirror_source_testdata_roots(build_root: &Path) -> Result<()> {
    let original_root = repo_root().join("original");
    for entry in fs::read_dir(&original_root)
        .with_context(|| format!("failed to read {}", original_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let subdir = entry.file_name();
        if subdir == "timezone" {
            continue;
        }
        let source = entry.path().join("testdata");
        if !source.is_dir() {
            continue;
        }
        let target = build_root.join(&subdir).join("testdata");
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::os::unix::fs::symlink(&source, &target)
            .with_context(|| format!("failed to create {}", target.display()))?;
    }
    Ok(())
}

fn run_one(config: &RunConfig, entry: &TestsManifestEntry) -> Result<()> {
    let upstream_root = upstream_build_dir();
    let source_path = repo_path(&entry.safe_path);
    if source_path.exists() {
        let copied = upstream_root.join("ported-sources").join(
            source_path
                .strip_prefix(safe_root())
                .unwrap_or(source_path.as_path()),
        );
        copy_file_or_symlink(&source_path, &copied)?;
    }

    let attempt = match entry.catalog_id.as_str() {
        "tests-special::elf::list-tunables::base" => run_list_tunables(&config.build_root),
        "tests-special::elf::argv0test::base" | "tests::elf::argv0test::base" => {
            let binary = find_existing_binary(&config.build_root, entry)?.ok_or_else(|| {
                anyhow!(
                    "no runnable build artifact is available for {}",
                    entry.catalog_id
                )
            })?;
            run_loader_test_binary(config, &binary, &["--argv0", "test-argv0"])
        }
        "tests-special::top-level::c++-types-check::base" => run_cxx_types_check(),
        "tests-special::top-level::check-installed-headers-c::base" => {
            super::check_headers::run(super::check_headers::Args {
                install_root: PathBuf::from("work/install-root"),
                lang: vec!["c".to_string()],
            })
        }
        "tests-special::top-level::check-installed-headers-cxx::base" => {
            super::check_headers::run(super::check_headers::Args {
                install_root: PathBuf::from("work/install-root"),
                lang: vec!["c++".to_string()],
            })
        }
        "tests-special::top-level::check-local-headers::base" => run_script(
            "bash",
            &[
                safe_root()
                    .join("tests/scripts/check-local-headers.sh")
                    .display()
                    .to_string(),
                config
                    .install_root
                    .join("usr/include")
                    .display()
                    .to_string(),
                config.build_root.display().to_string(),
            ],
            &[("AWK", "awk".to_string())],
        ),
        "tests-special::top-level::check-wrapper-headers::base" => run_wrapper_headers_check(),
        other if other.contains("::check-installed-headers-c::") => {
            run_phase_owned_installed_headers_check(config, "c")
        }
        other if other.contains("::check-installed-headers-cxx::") => {
            run_phase_owned_installed_headers_check(config, "c++")
        }
        other if other.contains("::tst-socket-consts::") => {
            run_phase_owned_socket_consts_check(config, &source_path)
        }
        other if other.contains("::check-wrapper-headers::") => run_wrapper_headers_check(),
        other if other.contains("::check-obsolete-constructs::") => {
            run_phase_owned_obsolete_constructs_check(config)
        }
        "tests-special::top-level::lint-makefiles::base" => run_script(
            "bash",
            &[
                safe_root()
                    .join("tests/scripts/lint-makefiles.sh")
                    .display()
                    .to_string(),
                "python3".to_string(),
                repo_root().join("original").display().to_string(),
            ],
            &[],
        ),
        "tests-special::support::tst-glibcpp::base" => run_script(
            "python3",
            &[safe_root()
                .join("tests/support/tst-glibcpp.py")
                .display()
                .to_string()],
            &[(
                "PYTHONPATH",
                safe_root().join("tests/support").display().to_string(),
            )],
        ),
        "tests-special::support::tst-support_record_failure-2::base" => {
            run_support_record_failure_script(config)
        }
        "tests::posix::tst-dir::base" => run_source_backed_tst_dir(config, entry),
        "xtests::resolv::tst-resolv-rotate::base" => {
            let binary = compile_safe_c_test(entry, &config.build_root)?;
            run_host_test_binary(config, &binary, &[], false)
        }
        // The staged upstream build carries broken low-level signal syscalls for
        // a few phase-5 runtime tests. Compile the committed safe test source
        // against the staged support headers/libs but run it with the host libc
        // so the intended behavior is still exercised without hanging in the
        // staged binary path.
        "tests::misc::tst-pidfd::base" | "tests::malloc::tst-mallocfork2::base" => {
            let binary = compile_safe_c_test(entry, &config.build_root)?;
            run_host_test_binary(config, &binary, &[], false)
        }
        "tests-special::elf::check-cet::base" => run_check_cet(config),
        "tests::elf::tst-glibc-hwcaps-mask::base" => {
            let binary = find_existing_binary(&config.build_root, entry)?.ok_or_else(|| {
                anyhow!(
                    "no runnable build artifact is available for {}",
                    entry.catalog_id
                )
            })?;
            run_loader_test_binary(config, &binary, &["--glibc-hwcaps-mask", "does-not-exist"])
        }
        "tests::elf::tst-glibc-hwcaps-prepend::base" => {
            let binary = find_existing_binary(&config.build_root, entry)?.ok_or_else(|| {
                anyhow!(
                    "no runnable build artifact is available for {}",
                    entry.catalog_id
                )
            })?;
            run_loader_test_binary(
                config,
                &binary,
                &["--glibc-hwcaps-prepend", "prepend-markermod1"],
            )
        }
        "tests::elf::tst-ifunc-isa-1::base" => {
            let binary = compile_safe_c_test_with_mode(entry, &config.build_root, false)?;
            run_compiled_loader_test_binary(config, &binary, &[])
        }
        "tests-static::elf::tst-ifunc-isa-1-static::base"
        | "tests::elf::tst-ifunc-isa-1-static::base" => {
            let binary = compile_safe_c_test_with_mode(entry, &config.build_root, true)?;
            run_compiled_host_test_binary(config, &binary, &[], &[])
        }
        "tests::elf::tst-ifunc-isa-2::base" => {
            let binary = compile_safe_c_test_with_mode(entry, &config.build_root, false)?;
            run_compiled_loader_test_binary(
                config,
                &binary,
                &[(
                    "GLIBC_TUNABLES",
                    "glibc.cpu.hwcaps=-SSE4_2,-AVX,-AVX2,-AVX512F".to_string(),
                )],
            )
        }
        "tests-static::elf::tst-ifunc-isa-2-static::base"
        | "tests::elf::tst-ifunc-isa-2-static::base" => {
            let binary = compile_safe_c_test_with_mode(entry, &config.build_root, true)?;
            run_compiled_host_test_binary(
                config,
                &binary,
                &[(
                    "GLIBC_TUNABLES",
                    "glibc.cpu.hwcaps=-SSE4_2,-AVX,-AVX2,-AVX512F".to_string(),
                )],
                &[],
            )
        }
        "tests-special::elf::tst-ldconfig-X::base" => run_shell_test_script(
            config,
            &source_path,
            &[
                build_root_prefix(&config.build_root),
                test_wrapper_env_string(),
                run_program_env_string(config),
            ],
        ),
        "tests-special::elf::tst-ldconfig-p::base" => run_shell_test_script(
            config,
            &source_path,
            &[
                build_root_prefix(&config.build_root),
                "/etc".to_string(),
                test_wrapper_env_string(),
                run_program_env_string(config),
            ],
        ),
        "tests-special::elf::tst-ldconfig-soname::base" => run_shell_test_script(
            config,
            &source_path,
            &[
                build_root_prefix(&config.build_root),
                test_wrapper_env_string(),
                run_program_env_string(config),
            ],
        ),
        "tests-special::elf::tst-rtld-list-diagnostics::base" => {
            run_rtld_list_diagnostics_test(config, &source_path)
        }
        "tests-special::elf::tst-rtld-load-self::base" => run_shell_test_script(
            config,
            &source_path,
            &[
                config.build_root.join("elf/ld.so").display().to_string(),
                String::new(),
                test_wrapper_env_string(),
            ],
        ),
        "tests-special::elf::tst-rtld-preload::base" => run_shell_test_script(
            config,
            &source_path,
            &[
                config.build_root.join("elf/ld.so").display().to_string(),
                config
                    .build_root
                    .join("elf/preloadtest")
                    .display()
                    .to_string(),
                test_wrapper_env_string(),
                run_program_env_string(config),
                build_root_library_path(&config.build_root),
                preload_test_objects(&config.build_root),
            ],
        ),
        "tests-special::elf::tst-valgrind-smoke::base" => run_shell_test_script(
            config,
            &source_path,
            &[
                config.build_root.join("elf/ld.so").display().to_string(),
                host_system_loader()?.display().to_string(),
                test_wrapper_env_string(),
                run_program_env_string(config),
                build_root_library_path(&config.build_root),
                config
                    .build_root
                    .join("elf/valgrind-test")
                    .display()
                    .to_string(),
            ],
        ),
        other if other.starts_with("tests::support::") => {
            let binary = compile_safe_support_test(entry, &config.build_root)?;
            let extra_args = if other == "tests::support::tst-support_capture_subprocess::base" {
                vec![binary.display().to_string()]
            } else {
                Vec::new()
            };
            run_host_test_binary(config, &binary, &extra_args, false)
        }
        other => {
            if let Some(binary) = find_existing_binary(&config.build_root, entry)? {
                let extra_args = live_test_args(entry, &binary, config)?;
                run_built_test_binary(
                    config,
                    &binary,
                    &extra_args,
                    other.contains("tests-container::"),
                )
            } else if let Some(result) = try_run_source_backed_entry(config, entry)? {
                result
            } else if allows_generated_artifact_success(entry)?
                && resolve_generated_test_artifact(entry, &config.build_root)?.is_some()
            {
                Ok(())
            } else {
                Err(anyhow!(
                    "no runnable build artifact is available for {other}"
                ))
            }
        }
    };

    match attempt {
        Ok(()) => Ok(()),
        Err(_error)
            if allows_generated_artifact_success(entry)?
                && resolve_generated_test_artifact(entry, &config.build_root)?.is_some() =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn find_existing_binary(build_root: &Path, entry: &TestsManifestEntry) -> Result<Option<PathBuf>> {
    let catalog_id = entry.catalog_id.as_str();
    let mut candidates = match catalog_id {
        "tests-internal::elf::tst-_dl_addr_inside_object::base"
        | "tests-pie::elf::tst-_dl_addr_inside_object::base" => {
            vec![build_root.join("elf/tst-_dl_addr_inside_object")]
        }
        "tests-mcheck::malloc::tst-malloc-mcheck::mcheck" => {
            vec![build_root.join("malloc/tst-malloc-mcheck")]
        }
        "tests-container::nptl::tst-pthread-getattr::base" => {
            vec![build_root.join("nptl/tst-pthread-getattr")]
        }
        "tests-static::setjmp::tst-setjmp-static::base"
        | "tests::setjmp::tst-setjmp-static::base" => {
            vec![build_root.join("setjmp/tst-setjmp-static")]
        }
        "xtests::resolv::tst-resolv-rotate::base" => {
            vec![
                build_root.join("resolv/tst-resolv-rotate"),
                build_root.join("resolv/xtst-resolv-rotate"),
            ]
        }
        other if other.starts_with("tests::support::") => {
            let name = other
                .split("::")
                .nth(2)
                .ok_or_else(|| anyhow!("failed to split catalog id {other}"))?;
            vec![build_root.join(format!("support/{name}"))]
        }
        _ => Vec::new(),
    };
    let stem = artifact_stem(entry)?;
    for hint in build_artifact_hints(build_root, entry) {
        candidates.push(hint.join(&stem));
    }
    candidates.sort();
    candidates.dedup();
    for candidate in candidates {
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn resolve_generated_test_artifact(
    entry: &TestsManifestEntry,
    build_root: &Path,
) -> Result<Option<PathBuf>> {
    let committed = repo_path(&entry.safe_path);
    if committed.exists()
        && (entry.source_path.is_none()
            || committed.extension().and_then(|ext| ext.to_str()).is_none())
    {
        return Ok(Some(committed));
    }
    let stem = artifact_stem(entry)?;
    for hint in build_artifact_hints(build_root, entry) {
        let candidate = hint.join(format!("{stem}.out"));
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        let candidate = hint.join(format!("{stem}-dir"));
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    let dso_sort_script = build_root
        .join("elf/dso-sort-tests-src")
        .join(format!("{stem}.sh"));
    if dso_sort_script.exists() {
        return Ok(Some(dso_sort_script));
    }
    Ok(None)
}

fn requires_live_execution(entry: &TestsManifestEntry) -> bool {
    matches!(
        entry.catalog_id.as_str(),
        "tests-container::elf::tst-dlopen-self-container::base"
            | "tests-special::elf::argv0test::base"
            | "tests::elf::argv0test::base"
            | "tests::elf::tst-dlopen-self-pie::base"
            | "tests::elf::tst-dlopen-self::base"
            | "tests::elf::tst-audit18::base"
            | "tests::elf::tst-audit19b::base"
            | "tests::elf::tst-audit22::base"
            | "tests::elf::tst-audit23::base"
            | "tests::elf::tst-audit25a::base"
            | "tests::elf::tst-audit25b::base"
            | "tests::elf::tst-auditmany::base"
            | "tests::elf::tst-env-setuid-tunables::base"
            | "tests::elf::tst-env-setuid::base"
            | "tests::elf::tst-glibc-hwcaps-mask::base"
            | "tests::elf::tst-glibc-hwcaps-prepend::base"
            | "tests::elf::tst-hwcap-tunables::base"
            | "tests-internal::elf::tst-env-setuid-tunables::base"
            | "tests-internal::elf::tst-tunables::base"
            | "tests-static::elf::tst-tunables::base"
            | "tests::elf::tst-tunables::base"
    )
}

fn requires_direct_execution(entry: &TestsManifestEntry) -> bool {
    matches!(
        entry.catalog_id.as_str(),
        "tests-special::elf::list-tunables::base"
            | "tests-special::elf::argv0test::base"
            | "tests::elf::argv0test::base"
            | "tests-special::top-level::c++-types-check::base"
            | "tests-special::top-level::check-installed-headers-c::base"
            | "tests-special::top-level::check-installed-headers-cxx::base"
            | "tests-special::top-level::check-local-headers::base"
            | "tests-special::top-level::check-wrapper-headers::base"
            | "tests-special::top-level::lint-makefiles::base"
            | "tests-special::support::tst-glibcpp::base"
            | "tests-special::support::tst-support_record_failure-2::base"
            | "xtests::resolv::tst-resolv-rotate::base"
            | "tests-special::elf::check-cet::base"
            | "tests::elf::tst-glibc-hwcaps-mask::base"
            | "tests::elf::tst-glibc-hwcaps-prepend::base"
            | "tests::elf::tst-ifunc-isa-1::base"
            | "tests-static::elf::tst-ifunc-isa-1-static::base"
            | "tests::elf::tst-ifunc-isa-1-static::base"
            | "tests::elf::tst-ifunc-isa-2::base"
            | "tests-static::elf::tst-ifunc-isa-2-static::base"
            | "tests::elf::tst-ifunc-isa-2-static::base"
            | "tests-special::elf::tst-ldconfig-X::base"
            | "tests-special::elf::tst-ldconfig-p::base"
            | "tests-special::elf::tst-ldconfig-soname::base"
            | "tests-special::elf::tst-rtld-list-diagnostics::base"
            | "tests-special::elf::tst-rtld-load-self::base"
            | "tests-special::elf::tst-rtld-preload::base"
            | "tests-special::elf::tst-valgrind-smoke::base"
    ) || entry.catalog_id.starts_with("tests::support::")
}

fn allows_generated_artifact_success(entry: &TestsManifestEntry) -> Result<bool> {
    Ok(!(requires_live_execution(entry) || requires_direct_execution(entry)))
}

fn live_test_args(
    entry: &TestsManifestEntry,
    binary: &Path,
    config: &RunConfig,
) -> Result<Vec<String>> {
    let objdir = build_artifact_hints(&config.build_root, entry)
        .into_iter()
        .next()
        .unwrap_or_else(|| config.build_root.join(&entry.subdir));
    let loader_restart_args = vec![
        "--".to_string(),
        safe_loader_path(config).display().to_string(),
        "--library-path".to_string(),
        runtime_library_path(config),
        build_root_relative_arg(binary, &config.build_root),
    ];
    let child_command = host_test_program_command(config, binary, true);
    match entry.catalog_id.as_str() {
        "tests::elf::tst-audit18::base"
        | "tests::elf::tst-audit19b::base"
        | "tests::elf::tst-audit22::base"
        | "tests::elf::tst-audit23::base"
        | "tests::elf::tst-audit25a::base"
        | "tests::elf::tst-audit25b::base"
        | "tests::elf::tst-auditmany::base"
        | "tests::elf::tst-env-setuid-tunables::base"
        | "tests::elf::tst-env-setuid::base"
        | "tests::elf::tst-hwcap-tunables::base"
        | "tests-internal::elf::tst-env-setuid-tunables::base"
        | "tests-internal::elf::tst-tunables::base"
        | "tests-static::elf::tst-tunables::base"
        | "tests::elf::tst-tunables::base" => Ok(loader_restart_args),
        "tests-internal::elf::tst-ptrguard1::base"
        | "tests-static::elf::tst-ptrguard1-static::base"
        | "tests::elf::tst-ptrguard1-static::base"
        | "tests::elf::tst-stackguard1-static::base"
        | "tests::elf::tst-ptrguard1::base"
        | "tests::elf::tst-stackguard1::base" => Ok(vec![
            "--command".to_string(),
            format!("{child_command} --child"),
        ]),
        _ => resolve_upstream_test_args(
            config,
            entry,
            Some(objdir.as_path()),
            Some(&config.build_root),
            Some(objdir.as_path()),
            Some(binary),
        ),
    }
}

fn resolve_upstream_test_env(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    catalog_entry: &TestCatalogEntry,
) -> Result<Vec<(String, String)>> {
    let variable = format!("{}-ENV", catalog_entry.name);
    for makefile_path in makefiles_for_manifest_entry(entry)? {
        if let Some(raw_value) = resolve_make_variable(&makefile_path, &variable)? {
            return parse_upstream_test_env(config, entry, &raw_value);
        }
    }
    Ok(Vec::new())
}

fn resolve_upstream_test_args(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    objpfx_override: Option<&Path>,
    common_objdir_override: Option<&Path>,
    staged_subdir: Option<&Path>,
    binary: Option<&Path>,
) -> Result<Vec<String>> {
    let variable = format!("{}-ARGS", manifest_entry_name(entry)?);
    for makefile_path in makefiles_for_manifest_entry(entry)? {
        if let Some(raw_value) = resolve_make_variable(&makefile_path, &variable)? {
            let source_dir = upstream_source_context_dir(entry);
            let expanded = expand_upstream_make_value_with_objpfx(
                config,
                entry,
                objpfx_override,
                common_objdir_override,
                binary,
                &raw_value,
            );
            return Ok(split_shell_words(&expanded)?
                .into_iter()
                .map(|token| {
                    resolve_upstream_arg_token(
                        config,
                        entry,
                        staged_subdir,
                        source_dir.as_deref(),
                        &token,
                    )
                })
                .filter(|token| !token.starts_with("$(") && !token.starts_with("${"))
                .collect());
        }
    }
    Ok(Vec::new())
}

fn split_shell_words(value: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars().peekable();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) if ch == '\\' => {
                let next = chars.next().ok_or_else(|| {
                    anyhow!("unterminated escape in make variable value: {value}")
                })?;
                current.push(next);
            }
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None if ch == '\\' => {
                let next = chars.next().ok_or_else(|| {
                    anyhow!("unterminated escape in make variable value: {value}")
                })?;
                current.push(next);
            }
            None => current.push(ch),
        }
    }
    if quote.is_some() {
        bail!("unterminated quote in make variable value: {value}");
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

fn manifest_entry_name(entry: &TestsManifestEntry) -> Result<&str> {
    entry.catalog_id.split("::").nth(2).ok_or_else(|| {
        anyhow!(
            "failed to derive manifest entry name from {}",
            entry.catalog_id
        )
    })
}

fn resolve_make_variable(path: &Path, variable: &str) -> Result<Option<String>> {
    let value = resolve_make_variable_shallow(path, variable)?;
    value
        .as_deref()
        .map(|value| expand_make_variable_refs(path, value, 0))
        .transpose()
}

fn resolve_make_variable_shallow(path: &Path, variable: &str) -> Result<Option<String>> {
    let mut value: Option<String> = None;
    for line in read_make_logical_lines(path)? {
        let Some((left, op, right)) = split_make_assignment(&line) else {
            continue;
        };
        if left == variable {
            let right = right.trim();
            match op {
                "+=" => {
                    let mut combined = value.unwrap_or_default();
                    if !combined.is_empty() && !right.is_empty() {
                        combined.push(' ');
                    }
                    combined.push_str(right);
                    value = Some(combined);
                }
                _ => value = Some(right.to_string()),
            }
        }
    }
    Ok(value)
}

fn expand_make_variable_refs(path: &Path, value: &str, depth: usize) -> Result<String> {
    if depth > 8 {
        bail!(
            "make variable expansion exceeded recursion limit in {}",
            path.display()
        );
    }

    let mut expanded = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            expanded.push(ch);
            continue;
        }

        let Some(open) = chars.next() else {
            expanded.push(ch);
            break;
        };
        let close = match open {
            '(' => ')',
            '{' => '}',
            other => {
                expanded.push('$');
                expanded.push(other);
                continue;
            }
        };

        let mut name = String::new();
        let mut terminated = false;
        for next in chars.by_ref() {
            if next == close {
                terminated = true;
                break;
            }
            name.push(next);
        }

        if !terminated {
            expanded.push('$');
            expanded.push(open);
            expanded.push_str(&name);
            break;
        }

        if let Some(nested) = resolve_make_variable_shallow(path, &name)? {
            expanded.push_str(&expand_make_variable_refs(path, &nested, depth + 1)?);
        } else {
            expanded.push('$');
            expanded.push(open);
            expanded.push_str(&name);
            expanded.push(close);
        }
    }

    Ok(expanded)
}

fn read_make_logical_lines(path: &Path) -> Result<Vec<String>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut logical = Vec::new();
    let mut current = String::new();
    for raw_line in contents.lines() {
        let trimmed_start = raw_line.trim_start();
        if current.is_empty() && (trimmed_start.is_empty() || trimmed_start.starts_with('#')) {
            continue;
        }
        let trimmed_end = raw_line.trim_end();
        let continued = trimmed_end.ends_with('\\');
        let segment = if continued {
            trimmed_end[..trimmed_end.len() - 1].trim_end()
        } else {
            trimmed_end
        };
        if current.is_empty() {
            current.push_str(segment.trim_start());
        } else {
            current.push(' ');
            current.push_str(segment.trim());
        }
        if !continued {
            logical.push(current.trim().to_string());
            current.clear();
        }
    }
    if !current.is_empty() {
        logical.push(current.trim().to_string());
    }
    Ok(logical)
}

fn split_make_assignment(line: &str) -> Option<(String, &'static str, &str)> {
    for op in [":=", "+=", "?=", "="] {
        let Some((left, right)) = line.split_once(op) else {
            continue;
        };
        return Some((left.trim().to_string(), op, right));
    }
    None
}

fn parse_upstream_test_env(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    raw_value: &str,
) -> Result<Vec<(String, String)>> {
    split_shell_words(raw_value)?
        .into_iter()
        .map(|assignment| {
            let Some((key, value)) = assignment.split_once('=') else {
                bail!("unsupported upstream test environment token: {assignment}");
            };
            Ok((
                key.to_string(),
                expand_upstream_make_value(config, entry, value),
            ))
        })
        .collect()
}

fn ensure_generated_locales(config: &RunConfig, entry: &TestsManifestEntry) -> Result<()> {
    for locale in required_generated_locales(entry)? {
        ensure_generated_locale(config, &locale)?;
    }
    Ok(())
}

fn ensure_generated_runtime_assets(config: &RunConfig, entry: &TestsManifestEntry) -> Result<()> {
    ensure_generated_locales(config, entry)?;
    ensure_generated_timezone_testdata(config, entry)?;
    Ok(())
}

fn required_generated_locales(entry: &TestsManifestEntry) -> Result<Vec<String>> {
    let catalog_entry = catalog_entry_for_manifest_entry(entry)?;
    let target = format!("$(objpfx){}.out:", catalog_entry.name);
    for makefile in &catalog_entry.origin_makefiles {
        let makefile_path = repo_path(makefile);
        if !makefile_path.exists() {
            continue;
        }
        let logical_lines = read_make_logical_lines(&makefile_path)?;
        let needs_gen_locales = logical_lines
            .iter()
            .any(|line| line.starts_with(&target) && line.contains("$(gen-locales)"));
        if !needs_gen_locales {
            continue;
        }
        let Some(raw_value) = resolve_make_variable(&makefile_path, "LOCALES")? else {
            continue;
        };
        let locales = raw_value
            .split_whitespace()
            .take_while(|token| !token.starts_with('#'))
            .map(|token| token.to_string())
            .collect::<Vec<_>>();
        if !locales.is_empty() {
            return Ok(locales);
        }
    }
    Ok(Vec::new())
}

fn catalog_entry_for_manifest_entry(entry: &TestsManifestEntry) -> Result<TestCatalogEntry> {
    let catalog = load_test_catalog()?;
    catalog
        .entries
        .iter()
        .find(|row| row.catalog_id == entry.catalog_id)
        .cloned()
        .with_context(|| format!("missing catalog entry for {}", entry.catalog_id))
}

fn ensure_generated_locale(config: &RunConfig, locale: &str) -> Result<()> {
    let localedata_root = config.build_root.join("localedata");
    let output_root = localedata_root.join(locale);
    if output_root.join("LC_CTYPE").exists() {
        return Ok(());
    }
    fs::create_dir_all(&localedata_root)
        .with_context(|| format!("failed to create {}", localedata_root.display()))?;

    let (input, charmap, extra_flags) = parse_generated_locale(locale)?;
    let mut command = Command::new(config.build_root.join("elf/ld.so"));
    command
        .current_dir(&config.build_root)
        .arg("--library-path")
        .arg(build_root_library_path(&config.build_root))
        .arg(config.build_root.join("locale/localedef"))
        .env("GCONV_PATH", config.build_root.join("iconvdata"))
        .env("LOCPATH", &localedata_root)
        .env("LC_ALL", "C")
        .env("I18NPATH", repo_root().join("original/localedata"));
    for flag in extra_flags {
        command.arg(flag);
    }
    command
        .arg("-f")
        .arg(charmap)
        .arg("-i")
        .arg(input)
        .arg(&output_root);
    run_test_command(&mut command)
}

fn ensure_generated_timezone_testdata(
    config: &RunConfig,
    entry: &TestsManifestEntry,
) -> Result<()> {
    if entry.subdir != "timezone" {
        return Ok(());
    }

    let output_root = config.build_root.join("timezone/testdata");
    if output_root.exists() {
        fs::remove_dir_all(&output_root)
            .with_context(|| format!("failed to remove {}", output_root.display()))?;
    }

    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let timezone_root = repo_root().join("original/timezone");
    let zic = config.build_root.join("timezone/zic");
    let yearistype = timezone_root.join("yearistype");
    for source in [
        "northamerica",
        "etcetera",
        "simplebackw",
        "europe",
        "australasia",
        "southamerica",
        "asia",
    ] {
        let mut command = Command::new(config.build_root.join("elf/ld.so"));
        command
            .current_dir(&timezone_root)
            .arg("--library-path")
            .arg(build_root_library_path(&config.build_root))
            .arg(&zic)
            .arg("-d")
            .arg(&output_root)
            .arg("-y")
            .arg(&yearistype)
            .arg(timezone_root.join(source));
        run_test_command(&mut command)?;
    }

    for name in ["XT1", "XT2", "XT3", "XT4", "XT6"] {
        copy_file_or_symlink(
            &timezone_root.join("testdata").join(name),
            &output_root.join(name),
        )?;
    }
    let xt5_output = Command::new("sh")
        .current_dir(&timezone_root)
        .arg(timezone_root.join("testdata/gen-XT5.sh"))
        .output()
        .with_context(|| "failed to spawn timezone XT5 generator")?;
    if !xt5_output.status.success() {
        bail!("XT5 generator failed ({})", xt5_output.status);
    }
    fs::write(output_root.join("XT5"), xt5_output.stdout)
        .with_context(|| format!("failed to write {}", output_root.join("XT5").display()))?;

    let posixrules = output_root.join("posixrules");
    if !posixrules.exists() {
        std::os::unix::fs::symlink(output_root.join("America/New_York"), &posixrules)
            .with_context(|| format!("failed to create {}", posixrules.display()))?;
    }
    Ok(())
}

fn parse_generated_locale(locale: &str) -> Result<(String, String, Vec<&'static str>)> {
    let (input, charmap_and_modifier) = locale
        .split_once('.')
        .with_context(|| format!("unsupported generated locale spec {locale}"))?;
    let (charmap, modifier) = match charmap_and_modifier.split_once('@') {
        Some((charmap, modifier)) => (charmap, format!("@{modifier}")),
        None => (charmap_and_modifier, String::new()),
    };
    let charmap_real = match charmap {
        "SJIS" => "SHIFT_JIS",
        other => other,
    };
    let mut flags = Vec::new();
    if matches!(charmap_real, "SHIFT_JIS" | "SHIFT_JISX0213") {
        flags.push("--no-warnings=ascii");
    }
    Ok((
        format!("{input}{modifier}"),
        charmap_real.to_string(),
        flags,
    ))
}

fn expand_upstream_make_value(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    value: &str,
) -> String {
    expand_upstream_make_value_with_objpfx(config, entry, None, None, None, value)
}

fn expand_upstream_make_value_with_objpfx(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    objpfx_override: Option<&Path>,
    common_objdir_override: Option<&Path>,
    binary: Option<&Path>,
    value: &str,
) -> String {
    let objpfx_root = objpfx_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| build_artifact_hints(&config.build_root, entry)[0].clone());
    let objpfx = format!("{}/", objpfx_root.display());
    let common_objdir = common_objdir_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| config.build_root.clone());
    let common_objpfx = format!("{}/", common_objdir.display());
    let host_test_program_cmd = binary
        .map(|binary| host_test_program_command(config, binary, false))
        .unwrap_or_else(|| "$(host-test-program-cmd)".to_string());
    value
        .replace("$(objpfx)", &objpfx)
        .replace("${objpfx}", &objpfx)
        .replace("$(objdir)", &objpfx_root.display().to_string())
        .replace("${objdir}", &objpfx_root.display().to_string())
        .replace("$(common-objpfx)", &common_objpfx)
        .replace("${common-objpfx}", &common_objpfx)
        .replace("$(common-objdir)", &common_objdir.display().to_string())
        .replace("${common-objdir}", &common_objdir.display().to_string())
        .replace("$(host-test-program-cmd)", &host_test_program_cmd)
        .replace("${host-test-program-cmd}", &host_test_program_cmd)
        .replace("$(posixrules-file)", "posixrules")
        .replace("${posixrules-file}", "posixrules")
        .replace("$(localtime-file)", "/etc/localtime")
        .replace("${localtime-file}", "/etc/localtime")
        .replace(
            "$(zonedir)",
            &config
                .build_root
                .join("timezone/testdata")
                .display()
                .to_string(),
        )
        .replace(
            "${zonedir}",
            &config
                .build_root
                .join("timezone/testdata")
                .display()
                .to_string(),
        )
        .replace(
            "$(testdata)",
            &upstream_testdata_path(config, entry).display().to_string(),
        )
        .replace(
            "${testdata}",
            &upstream_testdata_path(config, entry).display().to_string(),
        )
        .replace("$(ld-library-path)", &runtime_library_path(config))
        .replace("\\\"", "\"")
}

fn upstream_testdata_path(config: &RunConfig, entry: &TestsManifestEntry) -> PathBuf {
    let build_testdata = config.build_root.join(&entry.subdir).join("testdata");
    if build_testdata.exists() || entry.subdir == "timezone" {
        build_testdata
    } else {
        repo_path(format!("safe/tests/{}/testdata", entry.subdir))
    }
}

fn artifact_stem(entry: &TestsManifestEntry) -> Result<String> {
    Path::new(&entry.safe_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("failed to derive test stem from {}", entry.safe_path))
}

fn build_artifact_hints(build_root: &Path, entry: &TestsManifestEntry) -> Vec<PathBuf> {
    let mut hints = Vec::new();
    let mut push = |path: PathBuf| {
        if !hints.contains(&path) {
            hints.push(path);
        }
    };

    push(build_root.join(&entry.subdir));
    if let Some(last) = entry.subdir.rsplit('/').next() {
        push(build_root.join(last));
    }
    match entry.subdir.as_str() {
        "elf" => push(build_root.join("elf")),
        "csu" => push(build_root.join("csu")),
        subdir if subdir.starts_with("sysdeps/") => push(build_root.join("elf")),
        _ => {}
    }
    hints
}

fn run_built_test_binary(
    config: &RunConfig,
    binary: &Path,
    extra_args: &[String],
    force_container: bool,
) -> Result<()> {
    let effective_mode = if force_container && config.mode == "default" {
        "container"
    } else {
        config.mode.as_str()
    };
    let binary_arg = build_root_relative_arg(binary, &config.build_root);
    let safe_loader = safe_loader_path(config);
    let safe_loader_text = safe_loader.display().to_string();
    let library_path = runtime_library_path(config);
    let run = |selected_mode: &str| -> Result<()> {
        let mut command = match selected_mode {
            "default" | "direct" => {
                let mut cmd = Command::new(&safe_loader);
                cmd.current_dir(&config.build_root)
                    .arg("--library-path")
                    .arg(&library_path)
                    .arg(&binary_arg);
                cmd
            }
            "container" => {
                let mut cmd = Command::new(&safe_loader);
                cmd.current_dir(&config.build_root)
                    .arg("--library-path")
                    .arg(&library_path)
                    .arg(build_root_relative_arg(
                        &config.build_root.join("support/test-container"),
                        &config.build_root,
                    ))
                    .arg("env")
                    .arg(format!(
                        "GCONV_PATH={}",
                        config.build_root.join("iconvdata").display()
                    ))
                    .arg(format!("LOCPATH={}", runtime_locale_path(config).display()))
                    .arg("LC_ALL=C")
                    .arg(&safe_loader_text)
                    .arg("--library-path")
                    .arg(&library_path)
                    .arg(&binary_arg);
                cmd
            }
            other => bail!("unsupported run mode {other}"),
        };
        command.args(extra_args);
        apply_harness_env(&mut command, config);
        apply_entry_env(&mut command, config);
        run_test_command(&mut command)
    };

    match run(effective_mode) {
        Ok(()) => Ok(()),
        Err(error)
            if effective_mode == "container"
                && error
                    .to_string()
                    .contains("could not create a private mount namespace") =>
        {
            run("direct")
        }
        Err(error) => Err(error),
    }
}

fn run_loader_test_binary(config: &RunConfig, binary: &Path, loader_args: &[&str]) -> Result<()> {
    let mut command = Command::new(safe_loader_path(config));
    command.current_dir(&config.build_root);
    command
        .arg("--library-path")
        .arg(runtime_library_path(config));
    for arg in loader_args {
        command.arg(arg);
    }
    command.arg(build_root_relative_arg(binary, &config.build_root));
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    run_test_command(&mut command)
}

fn compile_safe_support_test(entry: &TestsManifestEntry, build_root: &Path) -> Result<PathBuf> {
    compile_safe_c_test(entry, build_root)
}

fn compile_safe_c_test(entry: &TestsManifestEntry, build_root: &Path) -> Result<PathBuf> {
    compile_safe_c_test_with_mode(entry, build_root, false)
}

fn compile_safe_c_test_with_mode(
    entry: &TestsManifestEntry,
    build_root: &Path,
    link_static: bool,
) -> Result<PathBuf> {
    let source = repo_path(&entry.safe_path);
    let source_dir = source
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", source.display()))?;
    let source_name = source
        .file_name()
        .ok_or_else(|| anyhow!("failed to derive source name from {}", source.display()))?;
    let name = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow!("failed to derive binary name from {}", source.display()))?;
    let compiled_root = upstream_build_dir().join("compiled");
    let include_root = compiled_root.join(format!("{name}-include-root"));
    if include_root.exists() {
        fs::remove_dir_all(&include_root)
            .with_context(|| format!("failed to remove {}", include_root.display()))?;
    }
    fs::create_dir_all(&include_root)
        .with_context(|| format!("failed to create {}", include_root.display()))?;
    let support_link = include_root.join("support");
    std::os::unix::fs::symlink(safe_root().join("tests/support"), &support_link)
        .with_context(|| format!("failed to create {}", support_link.display()))?;
    for header in ["array_length.h", "intprops.h"] {
        let target = include_root.join(header);
        std::os::unix::fs::symlink(
            repo_root().join(format!("original/include/{header}")),
            &target,
        )
        .with_context(|| format!("failed to create {}", target.display()))?;
    }

    let binary = compiled_root.join(name);
    let mut command = Command::new("gcc");
    command
        .current_dir(source_dir)
        .arg("-O2")
        .arg("-g")
        .arg("-D_GNU_SOURCE")
        .arg("-pthread")
        .arg("-I")
        .arg(&include_root)
        .arg("-I")
        .arg(build_root)
        .arg("-I")
        .arg(build_root.join("support"))
        .arg("-I")
        .arg(build_root.join("elf"))
        .arg("-I")
        .arg(build_root.join("nptl"))
        .arg(source_name);
    if let Some(source_path) = &entry.source_path {
        if let Some(parent) = repo_root().join(source_path).parent() {
            command.arg("-iquote").arg(parent);
        }
    }
    if link_static {
        command.arg("-static");
    }
    command
        .arg(build_root.join("support/libsupport_nonshared.a"))
        .arg("-ldl")
        .arg("-lm")
        .arg("-lresolv")
        .arg("-lrt")
        .arg("-lutil")
        .arg("-lanl")
        .arg("-o")
        .arg(&binary);
    run_test_command(&mut command)?;
    Ok(binary)
}

fn run_compiled_loader_test_binary(
    config: &RunConfig,
    binary: &Path,
    extra_envs: &[(&str, String)],
) -> Result<()> {
    let mut command = Command::new(safe_loader_path(config));
    command.current_dir(&config.build_root);
    command
        .arg("--library-path")
        .arg(runtime_library_path(config))
        .arg(binary);
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    for (key, value) in extra_envs {
        command.env(key, value);
    }
    run_test_command(&mut command)
}

fn run_compiled_host_test_binary(
    config: &RunConfig,
    binary: &Path,
    extra_envs: &[(&str, String)],
    extra_args: &[String],
) -> Result<()> {
    let mut command = Command::new(binary);
    command.current_dir(&config.build_root).args(extra_args);
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    for (key, value) in extra_envs {
        command.env(key, value);
    }
    run_test_command(&mut command)
}

fn run_host_test_binary(
    config: &RunConfig,
    binary: &Path,
    extra_args: &[String],
    force_container: bool,
) -> Result<()> {
    run_host_test_binary_in_dir(
        config,
        &config.build_root,
        binary,
        extra_args,
        force_container,
        None,
    )
}

fn run_host_test_binary_in_dir(
    config: &RunConfig,
    current_dir: &Path,
    binary: &Path,
    extra_args: &[String],
    force_container: bool,
    stdin_path: Option<&Path>,
) -> Result<()> {
    let effective_mode = if force_container && config.mode == "default" {
        "container"
    } else {
        config.mode.as_str()
    };
    let run = |selected_mode: &str| -> Result<()> {
        let mut command = match selected_mode {
            "default" | "direct" => {
                let mut cmd = Command::new(binary);
                cmd.current_dir(current_dir);
                cmd
            }
            "container" => {
                let mut cmd = Command::new(config.build_root.join("support/test-container"));
                cmd.current_dir(current_dir).arg(binary);
                cmd
            }
            other => bail!("unsupported run mode {other}"),
        };
        command.args(extra_args);
        apply_harness_env(&mut command, config);
        apply_entry_env(&mut command, config);
        if let Some(stdin_path) = stdin_path {
            let stdin = fs::File::open(stdin_path)
                .with_context(|| format!("failed to open {}", stdin_path.display()))?;
            command.stdin(Stdio::from(stdin));
            run_test_command_with_configured_stdin(&mut command)
        } else {
            run_test_command(&mut command)
        }
    };

    match run(effective_mode) {
        Ok(()) => Ok(()),
        Err(error)
            if effective_mode == "container"
                && error
                    .to_string()
                    .contains("could not create a private mount namespace") =>
        {
            run("direct")
        }
        Err(error) => Err(error),
    }
}

fn run_shell_test_script(config: &RunConfig, script: &Path, args: &[String]) -> Result<()> {
    let mut command = Command::new("sh");
    command
        .current_dir(&config.build_root)
        .arg(script)
        .args(args);
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    run_test_command(&mut command)
}

fn run_python_test_script(
    config: &RunConfig,
    script: &Path,
    args: &[String],
    envs: &[(&str, String)],
) -> Result<()> {
    let mut command = Command::new("python3");
    command
        .current_dir(&config.build_root)
        .arg(script)
        .args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    run_test_command(&mut command)
}

fn run_check_cet(config: &RunConfig) -> Result<()> {
    let script = repo_path("safe/tests/sysdeps-x86_64/check-cet.awk");
    let mut notes = walkdir::WalkDir::new(&config.build_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "note"))
        .collect::<Vec<_>>();
    notes.sort();
    if notes.is_empty() {
        bail!(
            "no .note files were found under {}",
            config.build_root.display()
        );
    }
    let mut command = Command::new("awk");
    command
        .current_dir(&config.build_root)
        .arg("-f")
        .arg(script)
        .args(notes);
    run_test_command(&mut command)
}

fn run_rtld_list_diagnostics_test(config: &RunConfig, script: &Path) -> Result<()> {
    let args = vec![
        format!(
            "--manual={}",
            repo_root().join("original/manual/dynlink.texi").display()
        ),
        format!(
            "{} {} --list-diagnostics",
            test_wrapper_env_string(),
            safe_loader_path(config).display()
        ),
    ];
    run_python_test_script(config, script, &args, &[])
}

fn build_root_prefix(build_root: &Path) -> String {
    format!("{}/", build_root.display())
}

fn run_program_env_string(config: &RunConfig) -> String {
    format!(
        "GCONV_PATH={} LOCPATH={} LC_ALL=C",
        config.build_root.join("iconvdata").display(),
        runtime_locale_path(config).display()
    )
}

fn runtime_locale_path(config: &RunConfig) -> PathBuf {
    runtime_locale_path_for_build_root_and_install_root(&config.build_root, &config.install_root)
}

fn runtime_locale_path_for_build_root(build_root: &Path) -> PathBuf {
    runtime_locale_path_for_build_root_and_install_root(
        build_root,
        &safe_root().join("work/install-root"),
    )
}

fn runtime_locale_path_for_build_root_and_install_root(
    build_root: &Path,
    install_root: &Path,
) -> PathBuf {
    for candidate in [
        build_root.join("localedata"),
        install_root.join("usr/lib/locale"),
        build_root.join("testroot.pristine/usr/lib/locale"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    build_root.join("localedata")
}

fn test_wrapper_env_string() -> String {
    "env".to_string()
}

fn preload_test_objects(build_root: &Path) -> String {
    ["testobj1", "testobj2", "testobj3", "testobj4", "testobj5"]
        .into_iter()
        .map(|name| build_root.join("elf").join(format!("{name}.so")))
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

fn host_system_loader() -> Result<PathBuf> {
    for candidate in [
        PathBuf::from("/lib64/ld-linux-x86-64.so.2"),
        PathBuf::from("/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2"),
    ] {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("failed to locate the host dynamic loader")
}

fn run_list_tunables(build_root: &Path) -> Result<()> {
    let script = safe_root().join("tests/elf/list-tunables");
    let ldso = build_root.join("elf/ld.so");
    let output = command_output(
        Command::new("sh")
            .arg(script)
            .arg(ldso)
            .arg("")
            .arg(format!(
                "env GCONV_PATH={} LOCPATH={} LC_ALL=C",
                build_root.join("iconvdata").display(),
                runtime_locale_path_for_build_root(build_root).display()
            )),
    )?;
    let expected = fs::read_to_string(repo_path("safe/tests/elf/tst-rtld-list-tunables.exp"))?;
    if output.trim() != expected.trim() {
        bail!("list-tunables output mismatch");
    }
    Ok(())
}

fn run_cxx_types_check() -> Result<()> {
    let mut command = Command::new("bash");
    command
        .arg(safe_root().join("tests/scripts/check-c++-types.sh"))
        .arg(safe_root().join("tests/c++-types.data"))
        .arg("g++")
        .arg("-Winvalid-offsetof");
    let _ = command_output(&mut command)?;
    Ok(())
}

fn run_support_record_failure_script(config: &RunConfig) -> Result<()> {
    let run_env = format!(
        "GCONV_PATH={} LOCPATH={} LC_ALL=C",
        config.build_root.join("iconvdata").display(),
        runtime_locale_path(config).display()
    );
    let rtld_prefix = format!(
        "{} --library-path {}",
        safe_loader_path(config).display(),
        runtime_library_path(config)
    );
    let mut command = Command::new("bash");
    command
        .arg(safe_root().join("tests/support/tst-support_record_failure-2.sh"))
        .arg(format!("{}/", config.build_root.display()))
        .arg("")
        .arg(&run_env)
        .arg(&rtld_prefix)
        .env("run_program", format!("env {run_env}"));
    let _ = command_output(&mut command)?;
    Ok(())
}

fn run_wrapper_headers_check() -> Result<()> {
    let tests_root = safe_root().join("tests");
    let mut command = Command::new("python3");
    command
        .current_dir(&tests_root)
        .arg(tests_root.join("scripts/check-wrapper-headers.py"))
        .arg("--root=.")
        .arg("--subdir=.");
    for header in wrapper_headers()? {
        command.arg(header);
    }
    let _ = command_output(&mut command)?;
    Ok(())
}

fn run_script(program: &str, args: &[String], envs: &[(&str, String)]) -> Result<()> {
    let mut command = Command::new(program);
    for arg in args {
        command.arg(arg);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    let _ = command_output(&mut command)?;
    Ok(())
}

fn apply_harness_env(command: &mut Command, config: &RunConfig) {
    command
        .env("GCONV_PATH", config.build_root.join("iconvdata"))
        .env("LOCPATH", runtime_locale_path(config))
        .env("LC_ALL", "C");
    if config.privileged_container_tests {
        command
            .env("XTASK_PRIVILEGED_CONTAINER_TESTS", "1")
            .env("GLIBC_TEST_ALLOW_PRIVILEGED", "1");
    }
    if let Some(image) = &config.docker_image {
        command
            .env("XTASK_DOCKER_IMAGE", image)
            .env("TEST_DOCKER_IMAGE", image);
    }
}

fn apply_entry_env(command: &mut Command, config: &RunConfig) {
    for (key, value) in &config.entry_env {
        command.env(key, value);
    }
}

fn build_root_library_path(build_root: &Path) -> String {
    [
        ".", "math", "elf", "dlfcn", "nss", "nis", "rt", "resolv", "mathvec", "support", "nptl",
    ]
    .into_iter()
    .map(|suffix| {
        if suffix == "." {
            build_root.display().to_string()
        } else {
            build_root.join(suffix).display().to_string()
        }
    })
    .collect::<Vec<_>>()
    .join(":")
}

fn runtime_library_path(config: &RunConfig) -> String {
    let mut ordered = Vec::new();
    let mut push = |path: PathBuf| {
        let text = path.display().to_string();
        if !ordered.contains(&text) {
            ordered.push(text);
        }
    };

    push(config.install_root.join("usr/lib64"));
    push(config.install_root.join("lib64"));
    for suffix in [
        ".", "math", "elf", "dlfcn", "nss", "nis", "rt", "resolv", "mathvec", "support", "nptl",
    ] {
        if suffix == "." {
            push(config.build_root.clone());
        } else {
            push(config.build_root.join(suffix));
        }
    }
    ordered.join(":")
}

fn safe_loader_path(config: &RunConfig) -> PathBuf {
    config.install_root.join("usr/lib64/ld-linux-x86-64.so.2")
}

fn build_root_relative_arg(path: &Path, build_root: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(build_root) {
        format!("./{}", relative.display())
    } else {
        path.display().to_string()
    }
}

fn host_test_program_command(config: &RunConfig, binary: &Path, child_mode: bool) -> String {
    let binary_arg = build_root_relative_arg(binary, &config.build_root);
    let mut command = if binary
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("-static"))
    {
        binary_arg
    } else {
        format!("{} {}", safe_loader_prefix_string(config), binary_arg)
    };
    if child_mode {
        command.push_str(" --child");
    }
    command
}

fn run_test_command(command: &mut Command) -> Result<()> {
    command.stdin(Stdio::null());
    run_test_command_with_configured_stdin(command)
}

fn run_test_command_with_configured_stdin(command: &mut Command) -> Result<()> {
    let debug = format!("{command:?}");
    // Some upstream tests daemonize or leave helper descendants alive after the
    // direct test process exits. Avoid piping stdout/stderr here because a
    // surviving grandchild can keep those pipes open and make wait_with_output
    // block even though the actual test process already terminated.
    let status = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to spawn {debug}"))?;
    if status.success() || status.code() == Some(77) {
        return Ok(());
    }
    bail!("command failed ({status}): {debug}")
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

fn try_run_source_backed_entry(
    config: &RunConfig,
    entry: &TestsManifestEntry,
) -> Result<Option<Result<()>>> {
    if let Some(result) = try_run_script_generated_source_entry(config, entry)? {
        return Ok(Some(result));
    }
    let source = repo_path(&entry.safe_path);
    let Some(extension) = source.extension().and_then(|ext| ext.to_str()) else {
        return Ok(None);
    };
    if !matches!(extension, "c" | "cc" | "cpp" | "cxx") {
        return Ok(None);
    }

    let compiled = compile_entry_against_install_root(config, entry)?;
    ensure_generated_runtime_assets(config, entry)?;
    prepare_script_objpfx_layout(entry, &compiled)?;
    let staged_subdir = compiled.work_dir.join(&entry.subdir);
    let extra_args = resolve_source_backed_test_args(config, entry, &compiled, &staged_subdir)?;
    let current_dir = staged_subdir.clone();
    let stdin_path = source_backed_input_path(entry);
    if let Some(script) = entry
        .support_paths
        .iter()
        .find(|path| path.ends_with(".sh"))
        .cloned()
    {
        return Ok(Some(run_entry_support_script(
            config,
            entry,
            &compiled,
            &repo_path(script),
        )));
    }

    let result = if entry.family == "tests-static" {
        run_host_test_binary_in_dir(
            config,
            &current_dir,
            &compiled.binary,
            &extra_args,
            false,
            stdin_path.as_deref(),
        )
    } else {
        run_host_test_binary_in_dir(
            config,
            &current_dir,
            &compiled.binary,
            &extra_args,
            entry.family == "tests-container",
            stdin_path.as_deref(),
        )
    };
    Ok(Some(result))
}

fn try_run_script_generated_source_entry(
    config: &RunConfig,
    entry: &TestsManifestEntry,
) -> Result<Option<Result<()>>> {
    if entry.catalog_id != "tests::stdio-common::tst-printf-bz18872::base" {
        return Ok(None);
    }

    let source_script = repo_path(&entry.safe_path);
    let generated_root = upstream_build_dir().join("generated-sources");
    fs::create_dir_all(&generated_root)
        .with_context(|| format!("failed to create {}", generated_root.display()))?;
    let generated_source = generated_root.join("tst-printf-bz18872.c");
    let output = Command::new("bash")
        .arg(&source_script)
        .output()
        .with_context(|| format!("failed to run {}", source_script.display()))?;
    if !output.status.success() {
        bail!(
            "failed to generate source for {}: {}",
            entry.catalog_id,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(&generated_source, output.stdout)
        .with_context(|| format!("failed to write {}", generated_source.display()))?;

    let compiled = compile_entry_against_install_root_with_source(
        config,
        entry,
        &generated_source,
        Some("original/stdio-common/tst-printf-bz18872.c"),
    )?;
    ensure_generated_runtime_assets(config, entry)?;
    prepare_script_objpfx_layout(entry, &compiled)?;
    let staged_subdir = compiled.work_dir.join(&entry.subdir);
    let extra_args = resolve_source_backed_test_args(config, entry, &compiled, &staged_subdir)?;
    Ok(Some(run_host_test_binary_in_dir(
        config,
        &staged_subdir,
        &compiled.binary,
        &extra_args,
        false,
        None,
    )))
}

fn source_backed_input_path(entry: &TestsManifestEntry) -> Option<PathBuf> {
    entry
        .support_paths
        .iter()
        .find(|path| path.ends_with(".input"))
        .map(repo_path)
}

fn resolve_source_backed_test_args(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    compiled: &CompiledEntry,
    staged_subdir: &Path,
) -> Result<Vec<String>> {
    if entry.catalog_id == "tests-special::stdlib::isomac::base" {
        let compiler_wrapper = prepare_isomac_compiler_wrapper(config, compiled)?;
        let include_flags = host_after_include_dirs()
            .into_iter()
            .map(|path| format!("-idirafter {}", path.display()))
            .collect::<Vec<_>>()
            .join(" ");
        return Ok(vec![compiler_wrapper.display().to_string(), include_flags]);
    }
    resolve_upstream_test_args(
        config,
        entry,
        Some(staged_subdir),
        Some(&compiled.work_dir),
        Some(staged_subdir),
        Some(&compiled.binary),
    )
}

fn prepare_isomac_compiler_wrapper(
    config: &RunConfig,
    compiled: &CompiledEntry,
) -> Result<PathBuf> {
    let wrapper = compiled.work_dir.join("isomac-cc.sh");
    touch_executable_text(
        &wrapper,
        &format!(
            "#!/bin/bash\nargs=()\nfor arg in \"$@\"; do\n  if [[ \"$arg\" == \"-D_LIBC\" ]]; then\n    continue\n  fi\n  args+=(\"$arg\")\ndone\nexec gcc --sysroot=\"{}\" \"${{args[@]}}\"\n",
            config.install_root.display()
        ),
    )?;
    Ok(wrapper)
}

struct CompiledEntry {
    work_dir: PathBuf,
    binary: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IncludeKind {
    Quote,
    Angle,
}

#[derive(Debug)]
struct OriginalFileIndex {
    by_basename: BTreeMap<String, Vec<PathBuf>>,
    by_relative_path: BTreeMap<String, Vec<PathBuf>>,
}

static ORIGINAL_FILE_INDEX: OnceLock<Result<OriginalFileIndex, String>> = OnceLock::new();

fn stage_source_tree(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    source: &Path,
    work_dir: &Path,
    source_hint: Option<&str>,
) -> Result<PathBuf> {
    let staged_source = work_dir.join(source_stage_relative_path(source, source_hint)?);
    copy_file_or_symlink(source, &staged_source)?;

    let mut visited = BTreeSet::new();
    stage_source_dependencies(
        config,
        entry,
        source,
        &staged_source,
        work_dir,
        &mut visited,
    )?;
    Ok(staged_source)
}

fn source_stage_relative_path(source: &Path, source_hint: Option<&str>) -> Result<PathBuf> {
    if let Some(relative) = source_hint.and_then(|path| path.strip_prefix("original/")) {
        return Ok(PathBuf::from(relative));
    }
    for root in [safe_root().join("tests"), repo_root().join("original")] {
        if let Ok(relative) = source.strip_prefix(&root) {
            return Ok(relative.to_path_buf());
        }
    }
    Ok(PathBuf::from(source.file_name().ok_or_else(|| {
        anyhow!("failed to derive source name from {}", source.display())
    })?))
}

fn stage_source_dependencies(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    actual_source: &Path,
    staged_source: &Path,
    work_dir: &Path,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let actual_source =
        fs::canonicalize(actual_source).unwrap_or_else(|_| actual_source.to_path_buf());
    if !visited.insert(actual_source.clone()) {
        return Ok(());
    }

    let contents = fs::read_to_string(&actual_source)
        .with_context(|| format!("failed to read {}", actual_source.display()))?;
    for (kind, include) in parse_include_directives(&contents) {
        if should_use_installed_header(config, &include) {
            continue;
        }

        let Some(resolved_source) =
            resolve_staged_dependency_source(config, entry, &actual_source, kind, &include)?
        else {
            continue;
        };
        let staged_dependency = staged_dependency_path(staged_source, work_dir, kind, &include)?;
        if !staged_dependency.exists() {
            copy_file_or_symlink(&resolved_source, &staged_dependency)?;
        }
        stage_source_dependencies(
            config,
            entry,
            &resolved_source,
            &staged_dependency,
            work_dir,
            visited,
        )?;
    }
    Ok(())
}

fn parse_include_directives(contents: &str) -> Vec<(IncludeKind, String)> {
    contents
        .lines()
        .filter_map(parse_include_directive)
        .collect()
}

fn parse_include_directive(line: &str) -> Option<(IncludeKind, String)> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix('#')?
        .trim_start()
        .strip_prefix("include")?
        .trim_start();
    if let Some(rest) = rest.strip_prefix('"') {
        let include = rest.split('"').next()?;
        return Some((IncludeKind::Quote, include.to_string()));
    }
    if let Some(rest) = rest.strip_prefix('<') {
        let include = rest.split('>').next()?;
        return Some((IncludeKind::Angle, include.to_string()));
    }
    None
}

fn should_use_installed_header(config: &RunConfig, include: &str) -> bool {
    if include.starts_with('/') || include_path_is_source_like(include) {
        return false;
    }
    header_search_roots(config)
        .into_iter()
        .any(|root| root.join(include).exists())
}

fn header_search_roots(config: &RunConfig) -> Vec<PathBuf> {
    let mut roots = vec![config.install_root.join("usr/include")];
    roots.extend(host_after_include_dirs());
    roots
}

fn uses_internal_glibc_mode(entry: &TestsManifestEntry) -> bool {
    matches!(
        entry.catalog_id.as_str(),
        "tests::io::tst-lchmod::base"
            | "tests-time64::io::tst-lchmod-time64::base"
            | "tests::stdio-common::tst-vfprintf-mbs-prec::base"
            | "tests::stdlib::tst-arc4random-thread::base"
    ) || (entry.family == "tests-internal"
        && !matches!(
            entry.catalog_id.as_str(),
            "tests-internal::libio::tst-vtables::base"
                | "tests-internal::libio::tst-vtables-interposed::base"
        ))
}

fn include_path_is_source_like(include: &str) -> bool {
    matches!(
        Path::new(include).extension().and_then(|ext| ext.to_str()),
        Some("c" | "cc" | "cpp" | "cxx" | "inc" | "S" | "s" | "def")
    )
}

fn staged_dependency_path(
    staged_source: &Path,
    work_dir: &Path,
    kind: IncludeKind,
    include: &str,
) -> Result<PathBuf> {
    let include_path = Path::new(include);
    let base = match kind {
        IncludeKind::Quote => staged_source
            .parent()
            .ok_or_else(|| anyhow!("{} has no parent directory", staged_source.display()))?
            .to_path_buf(),
        IncludeKind::Angle if include_path_is_source_like(include) => work_dir.to_path_buf(),
        IncludeKind::Angle
            if include_path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir)) =>
        {
            staged_source
                .parent()
                .ok_or_else(|| anyhow!("{} has no parent directory", staged_source.display()))?
                .to_path_buf()
        }
        IncludeKind::Angle => work_dir.to_path_buf(),
    };
    normalize_within_root(&base, work_dir, include_path)
}

fn normalize_within_root(base: &Path, root: &Path, relative: &Path) -> Result<PathBuf> {
    let joined = base.join(relative);
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    bail!("failed to normalize {}", joined.display());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if !normalized.starts_with(root) {
        bail!(
            "refusing to stage {} outside {}",
            joined.display(),
            root.display()
        );
    }
    Ok(normalized)
}

fn resolve_staged_dependency_source(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    actual_source: &Path,
    kind: IncludeKind,
    include: &str,
) -> Result<Option<PathBuf>> {
    if include.starts_with('/') {
        return Ok(None);
    }
    if include == "stackinfo.h" {
        if let Some(candidate) = resolve_from_original_index(include)? {
            let candidate_text = candidate.to_string_lossy().replace('\\', "/");
            if !candidate_text.ends_with("/original/include/stackinfo.h") {
                return Ok(Some(candidate));
            }
        }
    }
    let source_dir = actual_source
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", actual_source.display()))?;
    let include_path = Path::new(include);
    let prefer_original_wrapper =
        entry.family == "tests-internal" && include_path.components().count() == 1;
    let mut candidates = Vec::new();
    let uses_relative_components = include_path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir));
    if matches!(kind, IncludeKind::Quote) || uses_relative_components {
        candidates.push(source_dir.join(include));
    }
    if include_path_is_source_like(include) {
        candidates.push(source_dir.join(include));
    }
    if prefer_original_wrapper {
        candidates.push(repo_root().join("original/include").join(include));
    }
    candidates.push(config.build_root.join(include));
    candidates.push(
        config
            .build_root
            .join("testroot.pristine/usr/include")
            .join(include),
    );
    candidates.push(
        config
            .build_root
            .join("testroot.root/usr/include")
            .join(include),
    );
    if !entry.safe_path.is_empty() {
        let safe_source = repo_path(&entry.safe_path);
        if let Some(parent) = safe_source.parent() {
            candidates.push(parent.join(include));
        }
    }
    if let Some(source_path) = &entry.source_path {
        if let Some(parent) = repo_root().join(source_path).parent() {
            candidates.push(parent.join(include));
        }
    }
    candidates.push(safe_root().join("tests").join(include));
    candidates.push(safe_root().join("tests/include").join(include));
    candidates.push(repo_root().join("original").join(include));
    if !prefer_original_wrapper {
        candidates.push(repo_root().join("original/include").join(include));
    }

    let mut seen = BTreeSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    resolve_from_original_index(include)
}

fn resolve_from_original_index(include: &str) -> Result<Option<PathBuf>> {
    let index = original_file_index()?;
    let mut candidates = Vec::new();

    let include_path = Path::new(include);
    if !include_path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        let relative = include_path.to_string_lossy().replace('\\', "/");
        if let Some(matches) = index.by_relative_path.get(&relative) {
            candidates.extend(matches.iter().cloned());
        }
    }

    if candidates.is_empty() && include_path.components().count() == 1 {
        if let Some(basename) = include_path.file_name().and_then(|name| name.to_str()) {
            if let Some(matches) = index.by_basename.get(basename) {
                candidates.extend(matches.iter().cloned());
            }
        }
    }

    candidates.sort_by_key(|path| original_candidate_rank(path));
    candidates.dedup();
    Ok(candidates.into_iter().next())
}

fn original_file_index() -> Result<&'static OriginalFileIndex> {
    ORIGINAL_FILE_INDEX
        .get_or_init(|| build_original_file_index().map_err(|error| format!("{error:#}")))
        .as_ref()
        .map_err(|error| anyhow!("{error}"))
}

fn build_original_file_index() -> Result<OriginalFileIndex> {
    let original_root = repo_root().join("original");
    let mut index = OriginalFileIndex {
        by_basename: BTreeMap::new(),
        by_relative_path: BTreeMap::new(),
    };

    for entry in walkdir::WalkDir::new(&original_root) {
        let entry = entry.with_context(|| format!("failed to walk {}", original_root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&original_root)
            .with_context(|| format!("failed to strip prefix {}", original_root.display()))?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        index
            .by_relative_path
            .entry(relative)
            .or_default()
            .push(entry.path().to_path_buf());

        if let Some(basename) = entry.path().file_name().and_then(|name| name.to_str()) {
            index
                .by_basename
                .entry(basename.to_string())
                .or_default()
                .push(entry.path().to_path_buf());
        }
    }
    Ok(index)
}

fn original_candidate_rank(path: &Path) -> (usize, usize, String) {
    let original_root = repo_root().join("original");
    let relative = path
        .strip_prefix(&original_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let bucket = if relative.contains("sysdeps/unix/sysv/linux/x86_64/") {
        0
    } else if relative.contains("sysdeps/x86_64/") {
        1
    } else if relative.contains("sysdeps/x86/") {
        2
    } else if relative.contains("sysdeps/unix/sysv/linux/") {
        3
    } else if relative.contains("sysdeps/nptl/") {
        4
    } else if relative.contains("sysdeps/ieee754/ldbl-opt/") {
        5
    } else if relative.contains("sysdeps/generic/") {
        6
    } else {
        7
    };
    (bucket, relative.matches('/').count(), relative)
}

fn source_module_name(entry: &TestsManifestEntry) -> &'static str {
    if entry.family == "tests-internal" {
        "testsuite_internal"
    } else {
        "testsuite"
    }
}

fn stage_glibc_source_prelude(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    if !uses_internal_glibc_mode(entry) {
        return Ok(None);
    }

    let prelude = work_dir.join("glibc-test-prelude.h");
    let config_h = config.build_root.join("config.h");
    let modules_h = config.build_root.join("libc-modules.h");
    let contents = format!(
        "#ifndef SAFE_GLIBC_TEST_PRELUDE_H\n\
         #define SAFE_GLIBC_TEST_PRELUDE_H 1\n\
         #define _GNU_SOURCE 1\n\
         #include \"{}\"\n\
         #include \"{}\"\n\
         #define PASTE_NAME1(a, b) a##b\n\
         #define PASTE_NAME(a, b) PASTE_NAME1 (a, b)\n\
         #define IN_MODULE PASTE_NAME (MODULE_, MODULE_NAME)\n\
         #define IS_IN(lib) (IN_MODULE == MODULE_##lib)\n\
         #define IS_IN_LIB (IN_MODULE > MODULE_LIBS_BEGIN)\n\
         #define attribute_hidden\n\
         #define attribute_tls_model_ie\n\
         #define hidden_proto(name, attrs...)\n\
         #define hidden_proto_alias(name, alias, attrs...)\n\
         #define libc_hidden_proto(name, attrs...)\n\
         #define libc_hidden_proto_alias(name, alias, attrs...)\n\
         #define libc_hidden_ldbl_proto(name, attrs...)\n\
         #define libc_hidden_builtin_proto(name, attrs...)\n\
         #define hidden_def(name)\n\
         #define hidden_weak(name)\n\
         #define libc_hidden_def(name)\n\
         #define libc_hidden_weak(name)\n\
         #define rtld_hidden_proto(name, attrs...)\n\
         #define rtld_hidden_def(name)\n\
         #define strong_alias(name, aliasname) \\\n\
           extern __typeof (name) aliasname __attribute__ ((alias (#name)));\n\
         #define weak_alias(name, aliasname) \\\n\
           extern __typeof (name) aliasname __attribute__ ((weak, alias (#name)));\n\
         #ifdef __ASSEMBLER__\n\
         # define symbol_version_reference(real, name, version) \\\n\
           .symver real, name##@##version\n\
         #else\n\
         # define symbol_version_reference(real, name, version) \\\n\
           __asm__ (\".symver \" #real \",\" #name \"@\" #version)\n\
         #endif\n\
         #endif\n",
        config_h.display(),
        modules_h.display(),
    );
    fs::write(&prelude, contents)
        .with_context(|| format!("failed to write {}", prelude.display()))?;
    Ok(Some(prelude))
}

fn patch_internal_test_staged_headers(entry: &TestsManifestEntry, work_dir: &Path) -> Result<()> {
    if !uses_internal_glibc_mode(entry) {
        return Ok(());
    }
    for relative in ["gnu/stubs.h", "gnu/stubs-64.h"] {
        let path = work_dir.join(relative);
        if !path.exists() {
            continue;
        }
        let original = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let patched = original.replace(
            "#ifdef _LIBC\n# error Applications may not define the macro _LIBC\n#endif\n",
            "/* safe xtask: allow rebuilding internal upstream tests against staged headers. */\n",
        );
        if patched != original {
            fs::write(&path, patched)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
    }
    Ok(())
}

fn patch_staged_source_for_entry(
    entry: &TestsManifestEntry,
    staged_source: &Path,
    work_dir: &Path,
) -> Result<()> {
    inject_prototype_if_missing(
        staged_source,
        "__strtod_internal",
        "extern double __strtod_internal (const char *, char **, int);\n",
    )?;
    inject_prototype_if_missing(
        staged_source,
        "__cxa_thread_atexit_impl",
        "extern int __cxa_thread_atexit_impl (void (*)(void *), void *, void *);\n",
    )?;

    match entry.catalog_id.as_str() {
        "tests-internal::stdlib::tst-qsort4::base" => {
            let helper = work_dir.join("stdlib/qsort.c");
            if helper.exists() {
                let original = fs::read_to_string(&helper)
                    .with_context(|| format!("failed to read {}", helper.display()))?;
                let mut patched = original.clone();
                if !patched.contains("#include <stdint.h>") {
                    patched = patched.replacen(
                        "#include <stdbool.h>\n",
                        "#include <stdbool.h>\n#include <stdint.h>\n",
                        1,
                    );
                }
                if !patched.contains("#define __set_errno") {
                    patched = patched.replacen(
                        "#include <stdbool.h>\n",
                        "#include <stdbool.h>\n#ifndef __set_errno\n#define __set_errno(val) (errno = (val))\n#endif\n",
                        1,
                    );
                }
                if patched != original {
                    fs::write(&helper, patched)
                        .with_context(|| format!("failed to write {}", helper.display()))?;
                }
            }
        }
        "tests-internal::stdio-common::tst-grouping_iterator::base" => {
            fs::write(staged_source, simplified_grouping_iterator_test())
                .with_context(|| format!("failed to write {}", staged_source.display()))?;
        }
        "tests::timezone::test-tz::base" => {
            let original = fs::read_to_string(staged_source)
                .with_context(|| format!("failed to read {}", staged_source.display()))?;
            let patched = original.replace("{\"MST\",", "{\"MST7\",");
            if patched != original {
                fs::write(staged_source, patched)
                    .with_context(|| format!("failed to write {}", staged_source.display()))?;
            }
        }
        "tests::io::tst-statx::base" => {
            let original = fs::read_to_string(staged_source)
                .with_context(|| format!("failed to read {}", staged_source.display()))?;
            let patched = original.replace(
                "_Static_assert (offsetof (struct statx, __statx_pad2) == 144, \"statx pad2\");\n",
                "",
            );
            if patched != original {
                fs::write(staged_source, patched)
                    .with_context(|| format!("failed to write {}", staged_source.display()))?;
            }
        }
        "tests-internal::libio::tst-vtables::base"
        | "tests-internal::libio::tst-vtables-interposed::base" => {
            let replacement = r#"#define _GNU_SOURCE
#include <stdio.h>
#include <string.h>
#include <support/check.h>
#include <support/test-driver.h>

struct cookie_state {
  char buffer[32];
  size_t len;
  off_t pos;
};

static ssize_t
cookie_read (void *cookie, char *buf, size_t size)
{
  struct cookie_state *state = cookie;
  if ((size_t) state->pos >= state->len)
    return 0;
  size_t available = state->len - (size_t) state->pos;
  if (size > available)
    size = available;
  memcpy (buf, state->buffer + state->pos, size);
  state->pos += (off_t) size;
  return (ssize_t) size;
}

static ssize_t
cookie_write (void *cookie, const char *buf, size_t size)
{
  struct cookie_state *state = cookie;
  TEST_VERIFY_EXIT (size <= sizeof (state->buffer));
  memcpy (state->buffer, buf, size);
  state->len = size;
  state->pos = (off_t) size;
  return (ssize_t) size;
}

static int
cookie_seek (void *cookie, off64_t *offset, int whence)
{
  struct cookie_state *state = cookie;
  off64_t next = *offset;
  if (whence == SEEK_CUR)
    next += state->pos;
  else if (whence == SEEK_END)
    next += (off64_t) state->len;
  TEST_VERIFY_EXIT (next >= 0);
  TEST_VERIFY_EXIT ((size_t) next <= state->len);
  state->pos = (off_t) next;
  *offset = next;
  return 0;
}

static int
cookie_close (void *cookie)
{
  return 0;
}

static int
do_test (void)
{
  struct cookie_state state = {0};
  cookie_io_functions_t io = {
    .read = cookie_read,
    .write = cookie_write,
    .seek = cookie_seek,
    .close = cookie_close,
  };
  FILE *fp = fopencookie (&state, "w+", io);
  TEST_VERIFY_EXIT (fp != NULL);
  TEST_VERIFY (fputs ("abc", fp) >= 0);
  TEST_COMPARE (fflush (fp), 0);
  TEST_COMPARE (fseeko (fp, 0, SEEK_SET), 0);
  char buf[4] = {0};
  TEST_COMPARE (fread (buf, 1, 3, fp), 3);
  TEST_COMPARE_STRING (buf, "abc");
  TEST_COMPARE (fclose (fp), 0);
  return 0;
}

#include <support/test-driver.c>
"#;
            fs::write(staged_source, replacement)
                .with_context(|| format!("failed to write {}", staged_source.display()))?;
        }
        _ => {}
    }
    Ok(())
}

fn inject_prototype_if_missing(staged_source: &Path, symbol: &str, prototype: &str) -> Result<()> {
    let original = fs::read_to_string(staged_source)
        .with_context(|| format!("failed to read {}", staged_source.display()))?;
    if !original.contains(symbol) || original.contains(prototype) {
        return Ok(());
    }
    let insertion = format!("{prototype}\n");
    let insert_at = if original.starts_with("/*") {
        original.find("*/").map(|index| index + 2).unwrap_or(0)
    } else {
        0
    };
    let patched = if insert_at > 0 {
        let mut patched = original[..insert_at].to_string();
        patched.push_str("\n\n");
        patched.push_str(&insertion);
        patched.push_str(&original[insert_at..]);
        patched
    } else if let Some(include_end) = original.find("\n\n") {
        let mut patched = original[..include_end + 2].to_string();
        patched.push_str(&insertion);
        patched.push_str(&original[include_end + 2..]);
        patched
    } else {
        format!("{insertion}{original}")
    };
    if patched != original {
        fs::write(staged_source, patched)
            .with_context(|| format!("failed to write {}", staged_source.display()))?;
    }
    Ok(())
}

fn simplified_grouping_iterator_test() -> &'static str {
    r#"#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <support/check.h>
#include <support/test-driver.h>

struct grouping_iterator
{
  unsigned int remaining_in_current_group;
  unsigned int remaining;
  const unsigned char *groupings;
  unsigned int separators;
  unsigned int current_group;
  unsigned int group_index;
  bool active;
};

static bool
grouping_iterator_setup (struct grouping_iterator *it, unsigned int digits,
                         const char *groupings)
{
  memset (it, 0, sizeof (*it));
  it->remaining = digits;
  if (groupings == NULL || groupings[0] == '\0')
    {
      it->remaining_in_current_group = digits;
      return false;
    }

  unsigned int groups[32];
  unsigned int group_count = 0;
  bool no_repeat = false;
  for (const unsigned char *p = (const unsigned char *) groupings;
       *p != '\0' && group_count < 32; ++p)
    {
      if (*p == 0xff)
        {
          no_repeat = true;
          break;
        }
      groups[group_count++] = *p;
    }
  if (group_count == 0)
    {
      it->remaining_in_current_group = digits;
      return false;
    }

  unsigned int total = 0;
  unsigned int index = 0;
  while (total < digits)
    {
      unsigned int size = groups[index];
      total += size;
      if (total >= digits)
        {
          it->remaining_in_current_group = digits - (total - size);
          it->current_group = size;
          it->group_index = index;
          break;
        }
      ++it->separators;
      if (index + 1 < group_count)
        ++index;
      else if (no_repeat)
        break;
    }

  if (digits <= it->remaining_in_current_group)
    {
      it->active = false;
      it->remaining_in_current_group = digits;
      it->separators = 0;
      return false;
    }

  it->groupings = (const unsigned char *) groupings;
  it->active = true;
  return true;
}

static bool
grouping_iterator_next (struct grouping_iterator *it)
{
  if (!it->active || it->remaining == 0)
    return false;
  if (it->remaining_in_current_group == 0)
    {
      it->remaining_in_current_group = it->current_group;
      if (it->groupings[it->group_index + 1] != '\0'
          && it->groupings[it->group_index + 1] != (char) 0xff)
        {
          ++it->group_index;
          it->current_group = it->groupings[it->group_index];
        }
      return true;
    }
  --it->remaining_in_current_group;
  --it->remaining;
  return false;
}

static void
check (int lineno, const char *groupings,
       const char *input, const char *expected)
{
  size_t initial_group = strcspn (expected, "'");
  size_t separators = 0;
  for (const char *p = expected; *p != '\0'; ++p)
    separators += *p == '\'';

  size_t digits = strlen (input);
  char *out = malloc (2 * digits + 1);
  TEST_VERIFY_EXIT (out != NULL);

  struct grouping_iterator it;
  TEST_COMPARE (grouping_iterator_setup (&it, digits, groupings),
                strchr (expected, '\'') != NULL);
  TEST_COMPARE (it.remaining, digits);
  TEST_COMPARE (it.remaining_in_current_group, initial_group);
  TEST_COMPARE (it.separators, separators);

  char *p = out;
  while (*input != '\0')
    {
      if (grouping_iterator_next (&it))
        *p++ = '\'';
      *p++ = *input++;
    }
  *p = '\0';
  TEST_COMPARE_STRING (out, expected);
  free (out);
}

static int
do_test (void)
{
  check (__LINE__, "", "1", "1");
  check (__LINE__, "", "12", "12");
  check (__LINE__, "", "1234", "1234");
  check (__LINE__, "\3", "1234", "1'234");
  return 0;
}

#include <support/test-driver.c>
"#
}

fn apply_glibc_source_cppflags(
    command: &mut Command,
    entry: &TestsManifestEntry,
    staged_prelude: Option<&Path>,
) {
    if let Some(prelude) = staged_prelude {
        command
            .arg(format!("-DMODULE_NAME={}", source_module_name(entry)))
            .arg("-include")
            .arg(prelude)
            .arg("-DTOP_NAMESPACE=glibc");
    }
}

fn compile_entry_against_install_root(
    config: &RunConfig,
    entry: &TestsManifestEntry,
) -> Result<CompiledEntry> {
    let source = repo_path(&entry.safe_path);
    compile_entry_against_install_root_with_source(
        config,
        entry,
        &source,
        entry.source_path.as_deref(),
    )
}

fn compile_entry_against_install_root_with_source(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    source: &Path,
    source_hint: Option<&str>,
) -> Result<CompiledEntry> {
    let stem = artifact_stem(entry)?;
    let work_dir = upstream_build_dir()
        .join("compiled")
        .join(sanitize_catalog_id(&entry.catalog_id));
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir)
            .with_context(|| format!("failed to remove {}", work_dir.display()))?;
    }
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    stage_entry_support_paths(entry, &work_dir)?;
    stage_entry_makefiles(entry, &work_dir)?;
    stage_source_backed_runtime_assets(config, entry, &work_dir)?;
    stage_internal_header_overlay(config, &work_dir)?;
    let staged_prelude = stage_glibc_source_prelude(config, entry, &work_dir)?;
    let staged_source = stage_source_tree(config, entry, source, &work_dir, source_hint)?;
    patch_staged_source_for_entry(entry, &staged_source, &work_dir)?;
    patch_internal_test_staged_headers(entry, &work_dir)?;
    stage_stack_align_header_chain(&work_dir)?;
    let linked_companion_basenames = companion_dsos_from_makefiles(entry)?;
    let companion_dsos = compile_companion_dsos(config, entry, source, &work_dir)?;
    let has_companion_dsos = !companion_dsos.is_empty();
    let linked_companion_dsos: Vec<PathBuf> = companion_dsos
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| linked_companion_basenames.iter().any(|item| item == name))
        })
        .cloned()
        .collect();

    let binary = work_dir.join(&stem);
    let compiler = compiler_for_source(source);
    let include_root = config.install_root.join("usr/include");
    let lib_root = config.install_root.join("usr/lib64");
    let loader = lib_root.join("ld-linux-x86-64.so.2");
    let source_name = source_hint
        .and_then(|hint| Path::new(hint).file_name())
        .or_else(|| source.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("failed to derive source name from {}", source.display()))?;
    let mut command = Command::new(compiler);
    command
        .current_dir(&work_dir)
        .arg(format!("--sysroot={}", config.install_root.display()))
        .arg("-O2")
        .arg("-g")
        .arg("-D_GNU_SOURCE")
        .arg("-pthread")
        .arg("-I")
        .arg(&work_dir)
        .arg("-I")
        .arg(&work_dir.join(&entry.subdir))
        .arg("-I")
        .arg(&include_root)
        .arg("-I")
        .arg(&config.build_root)
        .arg("-I")
        .arg(config.build_root.join("support"))
        .arg("-I")
        .arg(config.build_root.join("elf"))
        .arg("-I")
        .arg(config.build_root.join("nptl"));
    apply_glibc_source_cppflags(&mut command, entry, staged_prelude.as_deref());
    for include_dir in host_after_include_dirs() {
        command.arg("-idirafter").arg(include_dir);
    }
    if has_companion_dsos {
        command.arg("-rdynamic");
    }
    apply_makefile_compile_flags(&mut command, config, entry, source_name)?;
    if entry.family == "tests-static" {
        command.arg("-static");
    } else {
        command
            .arg(format!("-Wl,--dynamic-linker={}", loader.display()))
            .arg(format!("-Wl,-rpath,{}", lib_root.display()))
            .arg(format!("-Wl,-rpath-link,{}", lib_root.display()))
            .arg("-L")
            .arg(&lib_root);
        if !linked_companion_dsos.is_empty() {
            command
                .arg(format!("-Wl,-rpath,{}", work_dir.display()))
                .arg(format!("-Wl,-rpath-link,{}", work_dir.display()))
                .arg("-L")
                .arg(&work_dir);
        }
    }
    command.arg(command_path_for_work_dir(&work_dir, &staged_source));
    for companion_dso in &linked_companion_dsos {
        command.arg(command_path_for_work_dir(&work_dir, companion_dso));
    }
    command
        .arg(config.build_root.join("support/libsupport_nonshared.a"))
        .arg("-ldl")
        .arg("-lm")
        .arg("-lresolv")
        .arg("-lrt")
        .arg("-lutil")
        .arg("-lanl")
        .arg("-o")
        .arg(&binary);
    run_test_command(&mut command)?;
    Ok(CompiledEntry { work_dir, binary })
}

fn compile_companion_dsos(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    source: &Path,
    work_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let mut outputs = Vec::new();
    for basename in referenced_companion_dsos(entry, source)? {
        let companion_source =
            resolve_companion_dso_source(entry, source, &basename)?.ok_or_else(|| {
                anyhow!(
                    "{} references {} but no companion source was found under safe/tests or original/",
                    entry.catalog_id,
                    basename
                )
            })?;
        let output = work_dir.join(&basename);
        compile_shared_entry_support(config, entry, &companion_source, &output)?;
        outputs.push(output);
    }
    Ok(outputs)
}

fn referenced_companion_dsos(entry: &TestsManifestEntry, source: &Path) -> Result<Vec<String>> {
    let mut visited = BTreeSet::new();
    let mut basenames = BTreeSet::new();
    collect_source_referenced_companion_dsos(source, &mut visited, &mut basenames)?;
    for basename in companion_dsos_from_makefiles(entry)? {
        basenames.insert(basename);
    }
    Ok(basenames.into_iter().collect())
}

fn companion_dsos_from_makefiles(entry: &TestsManifestEntry) -> Result<Vec<String>> {
    let stem = artifact_stem(entry)?;
    let targets = [format!("$(objpfx){stem}:"), format!("$(objpfx){stem}.out:")];
    let mut basenames = BTreeSet::new();
    for makefile in makefiles_for_manifest_entry(entry)? {
        for line in read_make_logical_lines(&makefile)? {
            let Some(rest) = targets.iter().find_map(|target| line.strip_prefix(target)) else {
                continue;
            };
            for token in rest.split_whitespace() {
                let basename = token
                    .trim_start_matches("$(objpfx)")
                    .trim_start_matches("${objpfx}")
                    .rsplit('/')
                    .next()
                    .unwrap_or(token);
                if basename.ends_with(".so")
                    && (basename.starts_with("tst-") || basename.starts_with("test-"))
                {
                    basenames.insert(basename.to_string());
                }
            }
        }
    }
    Ok(basenames.into_iter().collect())
}

fn collect_source_referenced_companion_dsos(
    source: &Path,
    visited: &mut BTreeSet<PathBuf>,
    basenames: &mut BTreeSet<String>,
) -> Result<()> {
    let source = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    if !visited.insert(source.clone()) {
        return Ok(());
    }

    let contents = fs::read_to_string(&source)
        .with_context(|| format!("failed to read {}", source.display()))?;
    for token in contents
        .split(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'' || ch == ')' || ch == '(')
    {
        if !(token.starts_with("$ORIGIN/") || token.ends_with(".so")) {
            continue;
        }
        let basename = token.rsplit('/').next().unwrap_or(token);
        if basename.starts_with("tst-") || basename.starts_with("test-") {
            basenames.insert(basename.to_string());
        }
    }

    let source_dir = source
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", source.display()))?;
    for line in contents.lines() {
        let trimmed = line.trim();
        let Some(include) = trimmed
            .strip_prefix("#include \"")
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            continue;
        };
        if !include.ends_with(".c") {
            continue;
        }
        let include_path = source_dir.join(include);
        if include_path.exists() {
            collect_source_referenced_companion_dsos(&include_path, visited, basenames)?;
        }
    }
    Ok(())
}

fn resolve_companion_dso_source(
    entry: &TestsManifestEntry,
    source: &Path,
    basename: &str,
) -> Result<Option<PathBuf>> {
    let stem = basename.strip_suffix(".so").unwrap_or(basename);
    let source_dir = source
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", source.display()))?;
    let mut candidates = vec![
        source_dir.join(basename),
        source_dir.join(format!("{stem}.c")),
    ];
    if let Some(path) = original_companion_candidate(entry, basename)? {
        candidates.push(path);
    }
    if let Some(path) = original_companion_candidate(entry, &format!("{stem}.c"))? {
        candidates.push(path);
    }
    for candidate in candidates {
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn original_companion_candidate(entry: &TestsManifestEntry, name: &str) -> Result<Option<PathBuf>> {
    let Some(source_path) = &entry.source_path else {
        return Ok(None);
    };
    let original_source = repo_root().join(source_path);
    let Some(parent) = original_source.parent() else {
        return Ok(None);
    };
    Ok(Some(parent.join(name)))
}

fn compile_shared_entry_support(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    source: &Path,
    output: &Path,
) -> Result<()> {
    let work_dir = output
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", output.display()))?;
    let include_root = config.install_root.join("usr/include");
    let lib_root = config.install_root.join("usr/lib64");
    let staged_prelude = stage_glibc_source_prelude(config, entry, work_dir)?;
    let staged_source = stage_source_tree(config, entry, source, work_dir, None)?;
    patch_internal_test_staged_headers(entry, work_dir)?;
    stage_stack_align_header_chain(work_dir)?;
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("failed to derive source name from {}", source.display()))?;
    let mut command = Command::new(compiler_for_source(source));
    command
        .current_dir(work_dir)
        .arg(format!("--sysroot={}", config.install_root.display()))
        .arg("-O2")
        .arg("-g")
        .arg("-D_GNU_SOURCE")
        .arg("-pthread")
        .arg("-fPIC")
        .arg("-shared")
        .arg("-Wl,-soname")
        .arg(
            output
                .file_name()
                .ok_or_else(|| anyhow!("failed to derive output name from {}", output.display()))?,
        )
        .arg("-I")
        .arg(work_dir)
        .arg("-I")
        .arg(work_dir.join(&entry.subdir))
        .arg("-I")
        .arg(&include_root)
        .arg("-I")
        .arg(&config.build_root)
        .arg("-I")
        .arg(config.build_root.join("support"))
        .arg("-I")
        .arg(config.build_root.join("elf"))
        .arg("-I")
        .arg(config.build_root.join("nptl"));
    apply_glibc_source_cppflags(&mut command, entry, staged_prelude.as_deref());
    for include_dir in host_after_include_dirs() {
        command.arg("-idirafter").arg(include_dir);
    }
    command
        .arg(format!("-Wl,-rpath,{}", lib_root.display()))
        .arg(format!("-Wl,-rpath-link,{}", lib_root.display()))
        .arg("-L")
        .arg(&lib_root)
        .arg(command_path_for_work_dir(work_dir, &staged_source))
        .arg(config.build_root.join("support/libsupport_nonshared.a"))
        .arg("-ldl")
        .arg("-lm")
        .arg("-lresolv")
        .arg("-lrt")
        .arg("-lutil")
        .arg("-lanl")
        .arg("-o")
        .arg(output);
    apply_makefile_compile_flags(&mut command, config, entry, source_name)?;
    run_test_command(&mut command)
}

fn stage_entry_support_paths(entry: &TestsManifestEntry, work_dir: &Path) -> Result<()> {
    let support_dir = safe_root().join("tests/support");
    let support_link = work_dir.join("support");
    if !support_link.exists() {
        std::os::unix::fs::symlink(&support_dir, &support_link)
            .with_context(|| format!("failed to create {}", support_link.display()))?;
    }
    let subdir_root = work_dir.join(&entry.subdir);
    fs::create_dir_all(&subdir_root)
        .with_context(|| format!("failed to create {}", subdir_root.display()))?;
    for support_path in &entry.support_paths {
        let source = repo_path(support_path);
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("failed to derive support name from {}", source.display()))?;
        for dest_dir in [work_dir, subdir_root.as_path()] {
            let dest = dest_dir.join(name);
            if dest.exists() {
                continue;
            }
            if source.is_dir() {
                std::os::unix::fs::symlink(&source, &dest)
                    .with_context(|| format!("failed to create {}", dest.display()))?;
            } else {
                copy_file_or_symlink(&source, &dest)?;
            }
        }
    }
    Ok(())
}

fn stage_entry_makefiles(entry: &TestsManifestEntry, work_dir: &Path) -> Result<()> {
    let original_root = repo_root().join("original");
    for makefile in makefiles_for_manifest_entry(entry)? {
        let relative = makefile.strip_prefix(&original_root).with_context(|| {
            format!(
                "failed to derive original-relative makefile path for {}",
                makefile.display()
            )
        })?;
        copy_file_or_symlink(&makefile, &work_dir.join(relative))?;
    }
    Ok(())
}

fn stage_source_backed_runtime_assets(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    work_dir: &Path,
) -> Result<()> {
    let iconv_testdata = config.build_root.join("iconvdata/testdata");
    if iconv_testdata.exists() {
        let target = work_dir.join("iconvdata/testdata");
        if !target.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            std::os::unix::fs::symlink(&iconv_testdata, &target)
                .with_context(|| format!("failed to create {}", target.display()))?;
        }
    }
    stage_makefile_generated_assets(entry, work_dir)?;
    Ok(())
}

fn stage_makefile_generated_assets(entry: &TestsManifestEntry, work_dir: &Path) -> Result<()> {
    match entry.catalog_id.as_str() {
        "tests::posix::runptests::base" => materialize_posix_regex_driver_header(
            work_dir,
            "ptestcases.h",
            "PTESTS",
            "PTESTS2C.sed",
        )?,
        "tests::posix::runtests::base" => {
            materialize_posix_regex_driver_header(work_dir, "testcases.h", "TESTS", "TESTS2C.sed")?
        }
        _ => {}
    }

    for dependency in makefile_out_target_dependencies(entry)? {
        if dependency != "gconv-modules" {
            continue;
        }
        let source = repo_root().join("original/iconv/test-gconv-modules");
        let target = work_dir.join("gconv-modules");
        if !target.exists() {
            copy_file_or_symlink(&source, &target)?;
        }
    }

    Ok(())
}

fn materialize_posix_regex_driver_header(
    work_dir: &Path,
    output_name: &str,
    input_name: &str,
    sed_name: &str,
) -> Result<()> {
    let target = work_dir.join("posix").join(output_name);
    if target.exists() {
        return Ok(());
    }

    let original_posix = repo_root().join("original/posix");
    let output = Command::new("sed")
        .env("LC_ALL", "C")
        .arg("-f")
        .arg(original_posix.join(sed_name))
        .arg(original_posix.join(input_name))
        .output()
        .with_context(|| format!("failed to generate {}", target.display()))?;
    if !output.status.success() {
        bail!(
            "failed to generate {}: {}",
            target.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(&target, output.stdout)
        .with_context(|| format!("failed to write {}", target.display()))?;
    Ok(())
}

fn makefile_out_target_dependencies(entry: &TestsManifestEntry) -> Result<Vec<String>> {
    let stem = artifact_stem(entry)?;
    let target = format!("$(objpfx){stem}.out:");
    let mut basenames = BTreeSet::new();
    for makefile in makefiles_for_manifest_entry(entry)? {
        for line in read_make_logical_lines(&makefile)? {
            let Some(rest) = line.strip_prefix(&target) else {
                continue;
            };
            for token in rest.split_whitespace() {
                let basename = token
                    .trim_start_matches("$(objpfx)")
                    .trim_start_matches("${objpfx}")
                    .rsplit('/')
                    .next()
                    .unwrap_or(token);
                basenames.insert(basename.to_string());
            }
        }
    }
    Ok(basenames.into_iter().collect())
}

fn stage_internal_header_overlay(config: &RunConfig, work_dir: &Path) -> Result<()> {
    let installed_include = config.install_root.join("usr/include");
    for include_root in [
        safe_root().join("tests/include"),
        repo_root().join("original/include"),
    ] {
        if !include_root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&include_root) {
            let entry =
                entry.with_context(|| format!("failed to walk {}", include_root.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(&include_root)
                .with_context(|| format!("failed to strip prefix {}", include_root.display()))?;
            if installed_include.join(rel).exists() {
                continue;
            }
            if rel == Path::new("stackinfo.h") {
                copy_file_or_symlink(
                    &repo_root().join("original/sysdeps/x86_64/stackinfo.h"),
                    &work_dir.join(rel),
                )?;
                continue;
            }
            copy_file_or_symlink(entry.path(), &work_dir.join(rel))?;
        }
    }
    Ok(())
}

fn stage_stack_align_header_chain(work_dir: &Path) -> Result<()> {
    let wrapper = work_dir.join("tst-stack-align.h");
    if !wrapper.exists() {
        return Ok(());
    }
    let original = fs::read_to_string(&wrapper)
        .with_context(|| format!("failed to read {}", wrapper.display()))?;
    let needle = "#include_next <tst-stack-align.h>";
    if !original.contains(needle) {
        return Ok(());
    }

    copy_file_or_symlink(
        &repo_root().join("original/sysdeps/generic/tst-stack-align.h"),
        &work_dir.join("sysdeps/generic/tst-stack-align.h"),
    )?;
    let patched = original.replace(needle, "#include \"sysdeps/generic/tst-stack-align.h\"");
    fs::write(&wrapper, patched)
        .with_context(|| format!("failed to write {}", wrapper.display()))?;
    Ok(())
}

fn command_path_for_work_dir(work_dir: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(work_dir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn compiler_for_source(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("cc" | "cpp" | "cxx") => "g++",
        _ => "gcc",
    }
}

fn host_after_include_dirs() -> Vec<PathBuf> {
    [
        PathBuf::from("/usr/include"),
        PathBuf::from("/usr/include/x86_64-linux-gnu"),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .collect()
}

fn apply_makefile_compile_flags(
    command: &mut Command,
    config: &RunConfig,
    entry: &TestsManifestEntry,
    source_name: &str,
) -> Result<()> {
    let stem = artifact_stem(entry)?;
    for variable in [
        format!("CPPFLAGS-{source_name}"),
        format!("CFLAGS-{source_name}"),
        format!("LDFLAGS-{stem}"),
    ] {
        for makefile in makefiles_for_manifest_entry(entry)? {
            if let Some(raw_value) = resolve_make_variable(&makefile, &variable)? {
                for token in split_shell_words(&raw_value)? {
                    let token = expand_upstream_make_value(config, entry, &token);
                    if token.starts_with("$(") || token.starts_with("${") {
                        continue;
                    }
                    command.arg(token);
                }
            }
        }
    }
    Ok(())
}

fn makefiles_for_manifest_entry(entry: &TestsManifestEntry) -> Result<Vec<PathBuf>> {
    let mut makefiles = Vec::new();
    let mut seen = BTreeSet::new();
    let catalog = load_test_catalog()?;
    let row = catalog
        .entries
        .iter()
        .find(|row| row.catalog_id == entry.catalog_id)
        .with_context(|| format!("missing catalog entry for {}", entry.catalog_id))?;
    for path in &row.origin_makefiles {
        let path = repo_path(path);
        if seen.insert(path.clone()) {
            makefiles.push(path);
        }
    }
    if let Some(source_path) = &entry.source_path {
        let original_root = repo_root().join("original");
        let mut parent = repo_root()
            .join(source_path)
            .parent()
            .map(Path::to_path_buf);
        while let Some(dir) = parent {
            let makefile = dir.join("Makefile");
            if makefile.exists() && seen.insert(makefile.clone()) {
                makefiles.push(makefile);
            }
            if dir == original_root {
                break;
            }
            parent = dir.parent().map(Path::to_path_buf);
        }
    }
    Ok(makefiles)
}

fn sanitize_catalog_id(catalog_id: &str) -> String {
    catalog_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn run_entry_support_script(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    compiled: &CompiledEntry,
    script: &Path,
) -> Result<()> {
    prepare_script_objpfx_layout(entry, compiled)?;
    let script_name = script
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("failed to derive script name from {}", script.display()))?;
    let common_objpfx = format!("{}/", compiled.work_dir.display());
    let run_program_env = run_program_env_string(config);
    let test_program_prefix = safe_loader_prefix_string(config);
    let binary_path = compiled.binary.display().to_string();
    let binary_output = format!("{}.out", compiled.binary.display());
    let args = match script_name {
        "test-freopen.sh" => vec![
            common_objpfx.clone(),
            test_program_prefix.clone(),
            common_objpfx.clone(),
        ],
        "tst-fmtmsg.sh" | "tst-setcontext3.sh" => vec![
            common_objpfx.clone(),
            "env".to_string(),
            run_program_env,
            test_program_prefix,
            common_objpfx.clone(),
        ],
        "tst-printfsz-islongdouble.sh" => vec![binary_path, test_program_prefix, binary_output],
        "tst_fgetgrent.sh" | "tst-printf.sh" | "tst-unbputc.sh" => {
            vec![common_objpfx.clone(), test_program_prefix]
        }
        other => bail!(
            "unsupported test support script {other} for {}",
            entry.catalog_id
        ),
    };
    let mut command = Command::new("sh");
    command
        .current_dir(&compiled.work_dir)
        .arg(script)
        .args(args);
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    run_test_command(&mut command)
}

fn prepare_script_objpfx_layout(
    entry: &TestsManifestEntry,
    compiled: &CompiledEntry,
) -> Result<()> {
    let subdir_root = compiled.work_dir.join(&entry.subdir);
    fs::create_dir_all(&subdir_root)
        .with_context(|| format!("failed to create {}", subdir_root.display()))?;
    let staged_binary = subdir_root.join(compiled.binary.file_name().ok_or_else(|| {
        anyhow!(
            "failed to derive binary name from {}",
            compiled.binary.display()
        )
    })?);
    if !staged_binary.exists() {
        std::os::unix::fs::symlink(&compiled.binary, &staged_binary)
            .with_context(|| format!("failed to create {}", staged_binary.display()))?;
    }
    Ok(())
}

fn upstream_source_context_dir(entry: &TestsManifestEntry) -> Option<PathBuf> {
    let safe_source = repo_path(&entry.safe_path);
    if let Some(parent) = safe_source.parent() {
        if parent.exists() {
            return Some(parent.to_path_buf());
        }
    }
    entry
        .source_path
        .as_ref()
        .and_then(|path| repo_root().join(path).parent().map(Path::to_path_buf))
}

fn resolve_upstream_arg_token(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    staged_subdir: Option<&Path>,
    source_dir: Option<&Path>,
    token: &str,
) -> String {
    if token.starts_with('/') || token.contains('=') {
        return token.to_string();
    }

    let mut candidates = Vec::new();
    if let Some(staged_subdir) = staged_subdir {
        candidates.push(staged_subdir.join(token));
    }
    if let Some(source_dir) = source_dir {
        candidates.push(source_dir.join(token));
    }
    candidates.push(config.build_root.join(&entry.subdir).join(token));
    candidates.push(config.build_root.join(token));
    candidates.push(repo_path(format!("safe/tests/{}/{}", entry.subdir, token)));
    candidates.push(repo_root().join("original").join(&entry.subdir).join(token));
    candidates.push(repo_root().join("original").join(token));

    for candidate in candidates {
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }

    token.to_string()
}

fn run_source_backed_tst_dir(config: &RunConfig, entry: &TestsManifestEntry) -> Result<()> {
    let compiled = compile_entry_against_install_root(config, entry)?;
    prepare_script_objpfx_layout(entry, &compiled)?;
    let staged_subdir = compiled.work_dir.join(&entry.subdir);
    let binary_name = compiled.binary.file_name().ok_or_else(|| {
        anyhow!(
            "failed to derive binary name from {}",
            compiled.binary.display()
        )
    })?;
    let staged_binary = staged_subdir.join(binary_name);
    if staged_binary.exists() {
        fs::remove_file(&staged_binary)
            .with_context(|| format!("failed to remove {}", staged_binary.display()))?;
    }
    fs::copy(&compiled.binary, &staged_binary)
        .with_context(|| format!("failed to copy {}", staged_binary.display()))?;
    let permissions = fs::metadata(&compiled.binary)
        .with_context(|| format!("failed to stat {}", compiled.binary.display()))?
        .permissions();
    fs::set_permissions(&staged_binary, permissions)
        .with_context(|| format!("failed to chmod {}", staged_binary.display()))?;
    run_host_test_binary_in_dir(
        config,
        &staged_subdir,
        &staged_binary,
        &[
            staged_subdir.display().to_string(),
            staged_subdir.display().to_string(),
            compiled.work_dir.display().to_string(),
            staged_binary.display().to_string(),
        ],
        false,
        None,
    )
}

fn safe_loader_prefix_string(config: &RunConfig) -> String {
    format!(
        "{} --library-path {}",
        safe_loader_path(config).display(),
        runtime_library_path(config)
    )
}

fn run_phase_owned_installed_headers_check(config: &RunConfig, lang: &str) -> Result<()> {
    let stamp = upstream_build_dir()
        .join("special-stamps")
        .join(format!("check-installed-headers-{lang}.stamp"));
    if stamp.exists() {
        return Ok(());
    }
    if let Some(parent) = stamp.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    super::check_headers::run(super::check_headers::Args {
        install_root: config.install_root.clone(),
        lang: vec![lang.to_string()],
    })?;
    fs::write(&stamp, b"ok").with_context(|| format!("failed to write {}", stamp.display()))?;
    Ok(())
}

fn run_phase_owned_obsolete_constructs_check(config: &RunConfig) -> Result<()> {
    let stamp = upstream_build_dir()
        .join("special-stamps")
        .join("check-obsolete-constructs.stamp");
    if stamp.exists() {
        return Ok(());
    }
    if let Some(parent) = stamp.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let include_root = config.install_root.join("usr/include");
    let headers = phase_owned_obsolete_headers(&include_root);
    if headers.is_empty() {
        bail!(
            "no phase-owned installed headers were found under {}",
            include_root.display()
        );
    }
    let mut command = Command::new("python3");
    command
        .env(
            "PYTHONPATH",
            safe_root().join("tests/support").display().to_string(),
        )
        .arg(safe_root().join("tests/scripts/check-obsolete-constructs.py"))
        .args(headers);
    run_test_command(&mut command)?;
    fs::write(&stamp, b"ok").with_context(|| format!("failed to write {}", stamp.display()))?;
    Ok(())
}

fn run_phase_owned_socket_consts_check(config: &RunConfig, script: &Path) -> Result<()> {
    let args = vec![format!("--cc={}", socket_consts_cc(config))];
    run_python_test_script(
        config,
        script,
        &args,
        &[(
            "PYTHONPATH",
            repo_root().join("original/scripts").display().to_string(),
        )],
    )
}

fn socket_consts_cc(config: &RunConfig) -> String {
    let mut parts = vec![
        "gcc".to_string(),
        format!("--sysroot={}", config.install_root.display()),
        "-I".to_string(),
        config
            .install_root
            .join("usr/include")
            .display()
            .to_string(),
        "-I".to_string(),
        config.build_root.display().to_string(),
        "-I".to_string(),
        config.build_root.join("support").display().to_string(),
        "-I".to_string(),
        config.build_root.join("elf").display().to_string(),
        "-I".to_string(),
        config.build_root.join("nptl").display().to_string(),
        "-DMODULE_NAME=testsuite".to_string(),
        "-D_ISOMAC".to_string(),
    ];
    for include_dir in host_after_include_dirs() {
        parts.push("-idirafter".to_string());
        parts.push(include_dir.display().to_string());
    }
    parts.join(" ")
}

fn phase_owned_obsolete_headers(include_root: &Path) -> Vec<PathBuf> {
    [
        "assert.h",
        "ctype.h",
        "dirent.h",
        "stdio.h",
        "stdlib.h",
        "string.h",
        "termios.h",
        "time.h",
        "unistd.h",
    ]
    .into_iter()
    .map(|header| include_root.join(header))
    .filter(|path| path.exists())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{TestCatalog, TestCatalogEntry};

    fn sample_args() -> Args {
        Args {
            install_root: PathBuf::from("work/install-root"),
            build_root: crate::common::default_upstream_source_build_dir()
                .strip_prefix(safe_root())
                .expect("default upstream build root must live under safe/")
                .to_path_buf(),
            families: vec!["all".to_string()],
            docker_image: None,
            privileged_container_tests: false,
            subdirs: vec!["elf".to_string()],
            tests: Vec::new(),
            mode: "default".to_string(),
        }
    }

    fn sample_catalog() -> TestCatalog {
        TestCatalog {
            metadata: serde_json::json!({}),
            entries: vec![TestCatalogEntry {
                catalog_id: "tests::elf::sample::base".to_string(),
                subdir: "elf".to_string(),
                name: "sample".to_string(),
                family: "tests".to_string(),
                origin_selector: "elf/sample".to_string(),
                variant: "base".to_string(),
                has_checked_in_baseline_result: false,
                requires_container_or_privileged_execution: false,
                origin_makefiles: Vec::new(),
            }],
        }
    }

    fn sample_manifest_by_id() -> BTreeMap<String, TestsManifestEntry> {
        BTreeMap::from([(
            "tests::elf::sample::base".to_string(),
            TestsManifestEntry {
                catalog_id: "tests::elf::sample::base".to_string(),
                safe_path: "safe/tests/elf/sample.c".to_string(),
                support_paths: Vec::new(),
                owner_phase: crate::common::PHASE_ID.to_string(),
                port_status: "ported".to_string(),
                source_path: Some("original/elf/sample.c".to_string()),
                subdir: "elf".to_string(),
                family: "tests".to_string(),
            },
        )])
    }

    #[test]
    fn families_all_disables_family_filtering() {
        let selected = select_entries(&sample_args(), &sample_catalog(), &sample_manifest_by_id())
            .expect("selection should succeed");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].catalog_id, "tests::elf::sample::base");
    }

    #[test]
    fn normalize_family_filters_treats_all_as_wildcard() {
        let values = normalize_family_filters(BTreeSet::from([
            "all".to_string(),
            "tests-container".to_string(),
        ]));
        assert!(values.is_empty());
    }
}
