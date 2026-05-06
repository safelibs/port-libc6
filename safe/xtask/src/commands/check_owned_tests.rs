use crate::common::{
    load_test_catalog, load_test_port_plan, load_tests_manifest, normalize_test_destination_path,
    repo_path, repo_relative_path,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const ALL_PORTED_ENTRY_COUNT: usize = 5_584;
const FINAL_ZERO_ENTRY_SENTINELS: [&str; 3] = [
    "safe/tests/nscd/.gitkeep",
    "safe/tests/po/.gitkeep",
    "safe/tests/manual/.gitkeep",
];
// These copied tests remain part of the committed ownership ledger, but their
// upstream harnesses rely on private glibc internals, custom NSS test DSOs,
// generated profiling/locale/GMP assets, or privileged namespace helpers that
// are not executable against work/install-root.
const NON_EXECUTABLE_UNDER_INSTALL_ROOT: [&str; 63] = [
    "tests-container::stdio-common::tst-popen3::base",
    "tests-container::stdlib::tst-system::base",
    "tests-container::string::tst-strerror::base",
    "tests-container::string::tst-strsignal::base",
    "tests-container::nss::tst-nss-compat1::base",
    "tests-container::nss::tst-nss-gai-hv2-canonname::base",
    "tests-container::nss::tst-nss-test3::base",
    "tests-container::nss::tst-reload1::base",
    "tests-container::nss::tst-reload2::base",
    "tests-time64::io::tst-lchmod-time64::base",
    "tests-time64::time::tst-clock2-time64::base",
    "tests-time64::time::tst-clock_settime-time64::base",
    "tests-time64::time::tst-settimeofday-time64::base",
    "tests-container::locale::tst-localedef-path-norm::base",
    "tests-container::localedata::tst-localedef-hardlinks::base",
    "tests-internal::dlfcn::tst-dlinfo-phdr::base",
    "tests-internal::nss::tst-field::base",
    "tests-internal::nss::tst-rfc3484-2::base",
    "tests-internal::nss::tst-rfc3484-3::base",
    "tests-internal::nss::tst-rfc3484::base",
    "tests-internal::posix::bug-regex20::base",
    "tests-internal::posix::bug-regex33::base",
    "tests-internal::posix::bug-regex5::base",
    "tests-internal::resolv::tst-ns_name_length_uncompressed::base",
    "tests-internal::resolv::tst-ns_rr_cursor::base",
    "tests-internal::resolv::tst-ns_samebinaryname::base",
    "tests-internal::resolv::tst-resolv-res_init-thread::base",
    "tests-internal::resolv::tst-resolv-res_init::base",
    "tests-internal::resolv::tst-resolv-res_ninit::base",
    "tests-special::gmon::tst-gmon-gprof::base",
    "tests-special::gmon::tst-gmon-pie-gprof::base",
    "tests-special::gmon::tst-gmon-static-gprof::base",
    "tests-special::gmon::tst-gmon-static-pie-gprof::base",
    "tests-special::gmon::tst-mcount-overflow-check::base",
    "tests-special::intl::tst-codeset::base",
    "tests-special::intl::tst-gettext2::base",
    "tests-special::intl::tst-gettext4::base",
    "tests-special::intl::tst-gettext6::base",
    "tests-special::intl::tst-gettext::base",
    "tests-special::intl::tst-translit::base",
    "tests-static::nss::tst-field::base",
    "tests-static::math::atest-exp2::base",
    "tests-static::math::atest-exp::base",
    "tests-static::math::atest-sincos::base",
    "tests-static::resolv::tst-ns_rr_cursor::base",
    "tests-static::resolv::tst-resolv-txnid-collision::base",
    "tests::dlfcn::tststatic2::base",
    "tests::inet::tst-deadline::base",
    "tests::io::tst-file_change_detection::base",
    "tests::io::tst-lchmod::base",
    "tests::math::atest-exp2::base",
    "tests::math::atest-exp::base",
    "tests::math::atest-sincos::base",
    "tests::nss::tst-nss-test1::base",
    "tests::nss::tst-nss-test2::base",
    "tests::nss::tst-nss-test4::base",
    "tests::nss::tst-nss-test5::base",
    "tests::nss::tst-nss-test_errno::base",
    "tests::resolv::tst-resolv-ai_idn-nolibidn2::base",
    "tests::socket::tst-sockaddr_un_set::base",
    "tests::time::tst-clock2::base",
    "tests::time::tst-clock_settime::base",
    "tests::time::tst-settimeofday::base",
];

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub owner_phase: Option<String>,
    #[arg(long, default_value_t = false)]
    pub all_ported: bool,
    #[arg(
        long = "install-root",
        visible_alias = "root",
        default_value = "work/install-root"
    )]
    pub install_root: PathBuf,
    #[arg(long, default_value = "work/original-build")]
    pub build_root: PathBuf,
    #[arg(long)]
    pub docker_image: Option<String>,
    #[arg(long, default_value_t = false)]
    pub privileged_container_tests: bool,
}

pub fn run(args: Args) -> Result<()> {
    let select_owner = args.owner_phase.is_some();
    if select_owner == args.all_ported {
        bail!("check-owned-tests requires exactly one of --owner-phase or --all-ported");
    }

    super::build::refresh_phase_outputs()?;
    super::stage_upstream_build::ensure_staged_upstream_build(
        Path::new("original"),
        &args.build_root,
    )?;

    let manifest = load_tests_manifest()?;
    let manifest_by_id = manifest
        .entries
        .iter()
        .map(|entry| (entry.catalog_id.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let plan = load_test_port_plan()?;
    let catalog = load_test_catalog()?;
    let catalog_ids = catalog
        .entries
        .iter()
        .map(|entry| entry.catalog_id.clone())
        .collect::<BTreeSet<_>>();

    let selected_ids = if let Some(owner_phase) = args.owner_phase.as_deref() {
        verify_owner_phase_completeness(owner_phase, &plan, &manifest_by_id, &catalog_ids)?;
        plan.entries
            .iter()
            .filter(|entry| entry.owner_phase == owner_phase)
            .map(|entry| entry.catalog_id.clone())
            .collect::<Vec<_>>()
    } else {
        verify_all_ported_completeness(&manifest, &manifest_by_id)?;
        manifest
            .entries
            .iter()
            .map(|entry| entry.catalog_id.clone())
            .collect::<Vec<_>>()
    };
    let executable_ids = selected_ids
        .into_iter()
        .filter(|catalog_id| is_executable_under_install_root(catalog_id))
        .collect::<Vec<_>>();

    super::run_original_tests::run(super::run_original_tests::Args {
        install_root: args.install_root,
        build_root: args.build_root,
        families: Vec::new(),
        docker_image: args.docker_image,
        privileged_container_tests: args.privileged_container_tests,
        subdirs: Vec::new(),
        tests: executable_ids,
        mode: "default".to_string(),
    })
}

fn verify_owner_phase_completeness(
    owner_phase: &str,
    plan: &crate::common::TestPortPlan,
    manifest_by_id: &BTreeMap<String, &crate::common::TestsManifestEntry>,
    catalog_ids: &BTreeSet<String>,
) -> Result<()> {
    let owned = plan
        .entries
        .iter()
        .filter(|entry| entry.owner_phase == owner_phase)
        .collect::<Vec<_>>();
    if owned.is_empty() {
        bail!("no test-port-plan entries are owned by {owner_phase}");
    }

    let mut failures = Vec::new();
    let mut expected_roots = BTreeSet::new();
    for entry in owned {
        if !catalog_ids.contains(&entry.catalog_id) {
            failures.push(format!(
                "{} is owned by {owner_phase} in the plan but missing from test-catalog.json",
                entry.catalog_id
            ));
            continue;
        }
        let Some(manifest_entry) = manifest_by_id.get(&entry.catalog_id) else {
            failures.push(format!(
                "{} is owned by {owner_phase} in the plan but missing from safe/tests/manifest.toml",
                entry.catalog_id
            ));
            continue;
        };
        if manifest_entry.owner_phase != owner_phase {
            failures.push(format!(
                "{} has manifest owner {}, expected {}",
                entry.catalog_id, manifest_entry.owner_phase, owner_phase
            ));
        }
        if manifest_entry.port_status != "ported" {
            failures.push(format!("{} is not marked ported", entry.catalog_id));
        }
        ensure_existing_path(
            &manifest_entry.safe_path,
            &entry.catalog_id,
            "committed safe path",
            &mut failures,
        );
        for support in &manifest_entry.support_paths {
            ensure_existing_path(support, &entry.catalog_id, "support path", &mut failures);
        }

        let normalized_destination = normalize_test_destination_path(&entry.destination_path);
        collect_expected_roots(&normalized_destination, &mut expected_roots);
        for asset in &entry.companion_assets {
            collect_expected_roots(&normalize_test_destination_path(asset), &mut expected_roots);
        }
    }

    for root in expected_roots {
        let path = repo_path(&root);
        if !path.exists() {
            failures.push(format!(
                "missing expected materialized test root {}",
                path.display()
            ));
        }
    }

    for zero_entry in &plan.zero_entry_subdirs {
        if zero_entry.owner_phase != owner_phase {
            continue;
        }
        let sentinel = format!(
            "{}/.gitkeep",
            normalize_test_destination_path(&zero_entry.destination_root)
        );
        let sentinel_path = repo_path(&sentinel);
        if !sentinel_path.exists() {
            failures.push(format!(
                "missing zero-entry sentinel {}",
                sentinel_path.display()
            ));
            continue;
        }
        if let Err(error) = ensure_git_tracked(&sentinel_path) {
            failures.push(format!("{error:#}"));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "check-owned-tests completeness failures for {owner_phase}:\n{}",
            failures.join("\n")
        )
    }
}

fn verify_all_ported_completeness(
    manifest: &crate::common::TestsManifest,
    manifest_by_id: &BTreeMap<String, &crate::common::TestsManifestEntry>,
) -> Result<()> {
    if manifest.entries.len() != ALL_PORTED_ENTRY_COUNT {
        bail!(
            "safe/tests/manifest.toml must contain exactly {} entries, found {}",
            ALL_PORTED_ENTRY_COUNT,
            manifest.entries.len()
        );
    }

    let mut failures = Vec::new();
    for entry in manifest.entries.iter() {
        if entry.port_status != "ported" {
            failures.push(format!("{} is not marked ported", entry.catalog_id));
        }
        ensure_existing_path(
            &entry.safe_path,
            &entry.catalog_id,
            "committed safe path",
            &mut failures,
        );
        for support in &entry.support_paths {
            ensure_existing_path(support, &entry.catalog_id, "support path", &mut failures);
        }
    }
    for sentinel in FINAL_ZERO_ENTRY_SENTINELS {
        let sentinel_path = repo_path(sentinel);
        if !sentinel_path.exists() {
            failures.push(format!(
                "missing final zero-entry sentinel {}",
                sentinel_path.display()
            ));
            continue;
        }
        if let Err(error) = ensure_git_tracked(&sentinel_path) {
            failures.push(format!("{error:#}"));
        }
    }

    if manifest_by_id.len() != manifest.entries.len() {
        bail!("safe/tests/manifest.toml contains duplicate catalog_id entries");
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "check-owned-tests --all-ported completeness failures:\n{}",
            failures.join("\n")
        )
    }
}

fn ensure_existing_path(
    logical_path: &str,
    catalog_id: &str,
    label: &str,
    failures: &mut Vec<String>,
) {
    let path = repo_path(logical_path);
    if !path.exists() {
        failures.push(format!(
            "{} is missing {} {}",
            catalog_id,
            label,
            path.display()
        ));
    }
}

fn collect_expected_roots(path: &str, roots: &mut BTreeSet<String>) {
    let Some(parent) = Path::new(path).parent() else {
        return;
    };
    let root = parent.display().to_string();
    if root.starts_with("safe/tests/") {
        roots.insert(root);
    }
}

fn ensure_git_tracked(path: &Path) -> Result<()> {
    let debug = format!("{}", path.display());
    let rel = repo_relative_path(path)?;
    let status = Command::new("git")
        .current_dir(repo_path("."))
        .arg("ls-files")
        .arg("--error-unmatch")
        .arg(rel)
        .status()
        .with_context(|| format!("failed to query git for {}", path.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{} must be tracked in git", debug))
    }
}

fn is_executable_under_install_root(catalog_id: &str) -> bool {
    if catalog_id.starts_with("tests-internal::nss::")
        || catalog_id.starts_with("tests-internal::resolv::")
        || catalog_id.starts_with("tests-static::nss::")
        || catalog_id.starts_with("tests-static::resolv::")
    {
        return false;
    }
    !NON_EXECUTABLE_UNDER_INSTALL_ROOT.contains(&catalog_id)
}
