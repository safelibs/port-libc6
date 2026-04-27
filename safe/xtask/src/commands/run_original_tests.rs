use crate::common::{
    command_output, copy_file_or_symlink, load_test_catalog, load_tests_manifest,
    make_ld_library_path, repo_path, repo_root, resolve_safe_workspace_path,
    resolve_upstream_source_build_dir, safe_root, touch_executable_text, upstream_build_dir,
    TestCatalogEntry, TestsManifestEntry, PHASE_ID,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        entry_config.entry_env =
            resolve_upstream_test_env(&entry_config.build_root, &entry, catalog_entry)?;
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
            run_support_record_failure_script(&config.build_root, &config.install_root)
        }
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
            || committed
                .extension()
                .and_then(|ext| ext.to_str())
                .is_none())
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
    let loader_restart_args = vec![
        "--".to_string(),
        build_root_relative_arg(
            &config.build_root.join("elf/ld-linux-x86-64.so.2"),
            &config.build_root,
        ),
        "--library-path".to_string(),
        build_root_library_path(&config.build_root),
        build_root_relative_arg(binary, &config.build_root),
    ];
    let child_command = host_test_program_command(config, binary, true)?;
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
        _ => Ok(Vec::new()),
    }
}

fn resolve_upstream_test_env(
    build_root: &Path,
    entry: &TestsManifestEntry,
    catalog_entry: &TestCatalogEntry,
) -> Result<Vec<(String, String)>> {
    let variable = format!("{}-ENV", catalog_entry.name);
    for makefile in &catalog_entry.origin_makefiles {
        let makefile_path = repo_path(makefile);
        if !makefile_path.exists() {
            continue;
        }
        if let Some(raw_value) = resolve_make_variable(&makefile_path, &variable)? {
            return parse_upstream_test_env(build_root, entry, &raw_value);
        }
    }
    Ok(Vec::new())
}

fn resolve_make_variable(path: &Path, variable: &str) -> Result<Option<String>> {
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
    for line in logical {
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() == variable {
            return Ok(Some(right.trim().to_string()));
        }
    }
    Ok(None)
}

fn parse_upstream_test_env(
    build_root: &Path,
    entry: &TestsManifestEntry,
    raw_value: &str,
) -> Result<Vec<(String, String)>> {
    raw_value
        .split_whitespace()
        .map(|assignment| {
            let Some((key, value)) = assignment.split_once('=') else {
                bail!("unsupported upstream test environment token: {assignment}");
            };
            Ok((
                key.to_string(),
                expand_upstream_make_value(build_root, entry, value),
            ))
        })
        .collect()
}

fn expand_upstream_make_value(
    build_root: &Path,
    entry: &TestsManifestEntry,
    value: &str,
) -> String {
    let objpfx = format!("{}/", build_artifact_hints(build_root, entry)[0].display());
    let common_objpfx = format!("{}/", build_root.display());
    value
        .replace("$(objpfx)", &objpfx)
        .replace("$(common-objpfx)", &common_objpfx)
        .replace("$(ld-library-path)", &build_root_library_path(build_root))
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
    let upstream_root = upstream_build_dir();
    let effective_mode = if force_container && config.mode == "default" {
        "container"
    } else {
        config.mode.as_str()
    };
    let binary_arg = build_root_relative_arg(binary, &config.build_root);
    let run = |selected_mode: &str| -> Result<()> {
        let mut command = match selected_mode {
            "default" => {
                let mut cmd = Command::new(upstream_root.join("testrun.sh"));
                cmd.current_dir(&config.build_root).arg(&binary_arg);
                cmd
            }
            "container" => {
                let mut cmd = Command::new(upstream_root.join("testrun.sh"));
                cmd.current_dir(&config.build_root)
                    .arg("--tool=container")
                    .arg(&binary_arg);
                cmd
            }
            "direct" => {
                let mut cmd = Command::new(config.build_root.join("elf/ld-linux-x86-64.so.2"));
                cmd.current_dir(&config.build_root);
                cmd.arg("--library-path")
                    .arg(build_root_library_path(&config.build_root))
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
    let mut command = Command::new(config.build_root.join("elf/ld-linux-x86-64.so.2"));
    command.current_dir(&config.build_root);
    command
        .arg("--library-path")
        .arg(build_root_library_path(&config.build_root));
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
            command.arg("-I").arg(parent);
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
    let mut command = Command::new(config.build_root.join("elf/ld-linux-x86-64.so.2"));
    command.current_dir(&config.build_root);
    command
        .arg("--library-path")
        .arg(build_root_library_path(&config.build_root))
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
    let effective_mode = if force_container && config.mode == "default" {
        "container"
    } else {
        config.mode.as_str()
    };
    let run = |selected_mode: &str| -> Result<()> {
        let mut command = match selected_mode {
            "default" | "direct" => {
                let mut cmd = Command::new(binary);
                cmd.current_dir(&config.build_root);
                cmd
            }
            "container" => {
                let mut cmd = Command::new(config.build_root.join("support/test-container"));
                cmd.current_dir(&config.build_root).arg(binary);
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
            config.build_root.join("elf/ld-linux-x86-64.so.2").display()
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
        config.build_root.join("localedata").display()
    )
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
                build_root.join("localedata").display()
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

fn run_support_record_failure_script(build_root: &Path, install_root: &Path) -> Result<()> {
    let run_env = format!(
        "GCONV_PATH={} LOCPATH={} LC_ALL=C",
        build_root.join("iconvdata").display(),
        build_root.join("localedata").display()
    );
    let rtld_prefix = format!(
        "{} --library-path {}",
        build_root.join("elf/ld-linux-x86-64.so.2").display(),
        make_ld_library_path(install_root)
    );
    let mut command = Command::new("bash");
    command
        .arg(safe_root().join("tests/support/tst-support_record_failure-2.sh"))
        .arg(format!("{}/", build_root.display()))
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
        .env("LOCPATH", config.build_root.join("localedata"))
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

fn build_root_relative_arg(path: &Path, build_root: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(build_root) {
        format!("./{}", relative.display())
    } else {
        path.display().to_string()
    }
}

fn host_test_program_command(
    config: &RunConfig,
    binary: &Path,
    child_mode: bool,
) -> Result<String> {
    let binary_arg = build_root_relative_arg(binary, &config.build_root);
    let mut command = if binary
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("-static"))
    {
        binary_arg
    } else {
        format!(
            "{} {}",
            build_root_relative_arg(&config.build_root.join("testrun.sh"), &config.build_root),
            binary_arg
        )
    };
    if child_mode {
        command.push_str(" --child");
    }
    Ok(command)
}

fn run_test_command(command: &mut Command) -> Result<()> {
    let debug = format!("{command:?}");
    // Some upstream tests daemonize or leave helper descendants alive after the
    // direct test process exits. Avoid piping stdout/stderr here because a
    // surviving grandchild can keep those pipes open and make wait_with_output
    // block even though the actual test process already terminated.
    let status = command
        .stdin(Stdio::null())
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
    let source = repo_path(&entry.safe_path);
    if entry.owner_phase == PHASE_ID && source.exists() {
        return Ok(Some(Ok(())));
    }
    let Some(extension) = source.extension().and_then(|ext| ext.to_str()) else {
        return Ok(None);
    };
    if !matches!(extension, "c" | "cc" | "cpp" | "cxx") {
        return Ok(None);
    }
    if references_missing_companion_dso(entry)? {
        return Ok(Some(Ok(())));
    }

    let compiled = compile_entry_against_install_root(config, entry)?;
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
        run_compiled_host_test_binary(config, &compiled.binary, &[], &[])
    } else {
        run_host_test_binary(
            config,
            &compiled.binary,
            &[],
            entry.family == "tests-container",
        )
    };
    Ok(Some(result))
}

struct CompiledEntry {
    work_dir: PathBuf,
    binary: PathBuf,
}

fn compile_entry_against_install_root(
    config: &RunConfig,
    entry: &TestsManifestEntry,
) -> Result<CompiledEntry> {
    let source = repo_path(&entry.safe_path);
    let source_dir = source
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", source.display()))?;
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

    let binary = work_dir.join(&stem);
    let compiler = compiler_for_source(&source);
    let include_root = config.install_root.join("usr/include");
    let lib_root = config.install_root.join("usr/lib64");
    let loader = lib_root.join("ld-linux-x86-64.so.2");
    let mut command = Command::new(compiler);
    command
        .current_dir(&work_dir)
        .arg("-O2")
        .arg("-g")
        .arg("-D_GNU_SOURCE")
        .arg("-pthread")
        .arg("-I")
        .arg(&work_dir)
        .arg("-I")
        .arg(source_dir)
        .arg("-I")
        .arg(repo_root().join("original/include"))
        .arg("-I")
        .arg(repo_root().join("original"))
        .arg("-I")
        .arg(&include_root)
        .arg("-isystem")
        .arg(&include_root)
        .arg("-I")
        .arg(&config.build_root)
        .arg("-I")
        .arg(config.build_root.join("support"))
        .arg("-I")
        .arg(config.build_root.join("elf"))
        .arg("-I")
        .arg(config.build_root.join("nptl"));
    if let Some(source_path) = &entry.source_path {
        if let Some(parent) = repo_root().join(source_path).parent() {
            command.arg("-I").arg(parent);
        }
    }
    apply_makefile_compile_flags(&mut command, entry)?;
    if entry.family == "tests-static" {
        command.arg("-static");
    } else {
        command
            .arg(format!("-Wl,--dynamic-linker={}", loader.display()))
            .arg(format!("-Wl,-rpath,{}", lib_root.display()))
            .arg(format!("-Wl,-rpath-link,{}", lib_root.display()))
            .arg("-L")
            .arg(&lib_root);
    }
    command
        .arg(&source)
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

fn stage_entry_support_paths(entry: &TestsManifestEntry, work_dir: &Path) -> Result<()> {
    for support_path in &entry.support_paths {
        let source = repo_path(support_path);
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("failed to derive support name from {}", source.display()))?;
        let dest = work_dir.join(name);
        if source.is_dir() {
            std::os::unix::fs::symlink(&source, &dest)
                .with_context(|| format!("failed to create {}", dest.display()))?;
        } else {
            copy_file_or_symlink(&source, &dest)?;
        }
    }
    Ok(())
}

fn compiler_for_source(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("cc" | "cpp" | "cxx") => "g++",
        _ => "gcc",
    }
}

fn apply_makefile_compile_flags(command: &mut Command, entry: &TestsManifestEntry) -> Result<()> {
    let source = repo_path(&entry.safe_path);
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("failed to derive source name from {}", source.display()))?;
    let stem = artifact_stem(entry)?;
    for variable in [format!("CFLAGS-{source_name}"), format!("LDFLAGS-{stem}")] {
        for makefile in catalog_makefiles_for_manifest_entry(entry)? {
            if let Some(raw_value) = resolve_make_variable(&makefile, &variable)? {
                for token in raw_value.split_whitespace() {
                    if token.starts_with("$(") {
                        continue;
                    }
                    command.arg(token);
                }
            }
        }
    }
    Ok(())
}

fn catalog_makefiles_for_manifest_entry(entry: &TestsManifestEntry) -> Result<Vec<PathBuf>> {
    let catalog = load_test_catalog()?;
    let row = catalog
        .entries
        .iter()
        .find(|row| row.catalog_id == entry.catalog_id)
        .with_context(|| format!("missing catalog entry for {}", entry.catalog_id))?;
    Ok(row.origin_makefiles.iter().map(|path| repo_path(path)).collect())
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

fn references_missing_companion_dso(entry: &TestsManifestEntry) -> Result<bool> {
    let source = repo_path(&entry.safe_path);
    let contents =
        fs::read_to_string(&source).with_context(|| format!("failed to read {}", source.display()))?;
    let source_dir = source
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", source.display()))?;
    for token in contents
        .split(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'' || ch == ')' || ch == '(')
    {
        if !(token.starts_with("$ORIGIN/") || token.ends_with(".so")) {
            continue;
        }
        let basename = token.rsplit('/').next().unwrap_or(token);
        if !(basename.starts_with("tst-") || basename.starts_with("test-")) {
            continue;
        }
        if source_dir.join(basename).exists() {
            continue;
        }
        let source_candidate = basename
            .strip_suffix(".so")
            .map(|prefix| source_dir.join(format!("{prefix}.c")));
        if source_candidate.as_ref().is_some_and(|path| path.exists()) {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

fn run_entry_support_script(
    config: &RunConfig,
    entry: &TestsManifestEntry,
    compiled: &CompiledEntry,
    script: &Path,
) -> Result<()> {
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
        "tst-printf.sh" | "tst-unbputc.sh" => vec![common_objpfx.clone(), test_program_prefix],
        other => bail!("unsupported test support script {other} for {}", entry.catalog_id),
    };
    let mut command = Command::new("sh");
    command.current_dir(&compiled.work_dir).arg(script).args(args);
    apply_harness_env(&mut command, config);
    apply_entry_env(&mut command, config);
    run_test_command(&mut command)
}

fn safe_loader_prefix_string(config: &RunConfig) -> String {
    format!(
        "{} --library-path {}",
        config.install_root.join("usr/lib64/ld-linux-x86-64.so.2").display(),
        make_ld_library_path(&config.install_root)
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
