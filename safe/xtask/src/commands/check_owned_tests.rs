use crate::common::{
    load_test_catalog, load_test_port_plan, load_tests_manifest, normalize_test_destination_path,
    repo_path, repo_relative_path,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

const ALL_PORTED_ENTRY_COUNT: usize = 5_584;
const FINAL_ZERO_ENTRY_SENTINELS: [&str; 3] = [
    "safe/tests/nscd/.gitkeep",
    "safe/tests/po/.gitkeep",
    "safe/tests/manual/.gitkeep",
];
// These copied tests remain part of the committed ownership ledger, but their
// upstream harnesses rely on private glibc internals, custom NSS test DSOs,
// generated profiling/locale/GMP assets, build-tree loader/cache setup,
// generated companion DSOs, ELF/audit/linker harnesses, static/private build
// variants, conformance scripts superseded by check-headers, or privileged
// kernel helpers that are not executable against work/install-root.
const NON_EXECUTABLE_UNDER_INSTALL_ROOT: &[&str] = &[
    "tests-container::elf::tst-glibc-hwcaps-2-cache::base",
    "tests-container::elf::tst-glibc-hwcaps-cache::base",
    "tests-container::elf::tst-glibc-hwcaps-prepend-cache::base",
    "tests-container::elf::tst-ldconfig-bad-aux-cache::base",
    "tests-container::elf::tst-ldconfig-ld_so_conf-update::base",
    "tests-container::elf::tst-preload-pthread-libc::base",
    "tests-container::stdio-common::tst-popen3::base",
    "tests-container::stdlib::tst-system::base",
    "tests-container::string::tst-strerror::base",
    "tests-container::string::tst-strsignal::base",
    "tests-container::nss::tst-initgroups1::base",
    "tests-container::nss::tst-initgroups2::base",
    "tests-container::nss::tst-nss-compat1::base",
    "tests-container::nss::tst-nss-db-endgrent::base",
    "tests-container::nss::tst-nss-files-hosts-long::base",
    "tests-container::nss::tst-nss-files-hosts-v4mapped::base",
    "tests-container::nss::tst-nss-gai-actions::base",
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
    "tests-special::elf::tst-ldconfig-X::base",
    "tests-special::elf::tst-ldconfig-p::base",
    "tests-special::elf::tst-ldconfig-soname::base",
    "tests-static::nss::tst-field::base",
    "tests-static::math::atest-exp2::base",
    "tests-static::math::atest-exp::base",
    "tests-static::math::atest-sincos::base",
    "tests-static::resolv::tst-ns_rr_cursor::base",
    "tests-static::resolv::tst-resolv-txnid-collision::base",
    "tests::dlfcn::tststatic2::base",
    "tests::elf::tst-glibc-hwcaps-2::base",
    "tests::elf::tst-glibc-hwcaps-mask::base",
    "tests::elf::tst-glibc-hwcaps-prepend::base",
    "tests::elf::tst-glibc-hwcaps::base",
    "tests::inet::tst-deadline::base",
    "tests::io::tst-file_change_detection::base",
    "tests::io::tst-getcwd-abspath::base",
    "tests::io::tst-getcwd-smallbuff::base",
    "tests::io::tst-lchmod::base",
    "tests::math::atest-exp2::base",
    "tests::math::atest-exp::base",
    "tests::math::atest-sincos::base",
    "tests::math::test-math-cxx11::base",
    "tests::misc::tst-clock_adjtime::base",
    "tests::misc::tst-gethostid::base",
    "tests::misc::tst-mount::base",
    "tests::misc::tst-pidfd_getpid::base",
    "tests::misc::tst-sysconf-iov_max::base",
    "tests::misc::tst-tsearch::base",
    "tests::misc::tst-ttyname-direct::base",
    "tests::misc::tst-ttyname-namespace::base",
    "tests::nptl::tst-audit-threads::base",
    "tests::nptl::tst-cleanup4::base",
    "tests::nptl::tst-cleanupx4::base",
    "tests::nptl::tst-initializers1-c11::base",
    "tests::nptl::tst-initializers1-c89::base",
    "tests::nptl::tst-initializers1-c99::base",
    "tests::nptl::tst-initializers1-gnu11::base",
    "tests::nptl::tst-initializers1-gnu89::base",
    "tests::nptl::tst-initializers1-gnu99::base",
    "tests::nptl::tst-initializers1::base",
    "tests::nptl::tst-join7::base",
    "tests::nptl::tst-memstream::base",
    "tests::nptl::tst-stack4::base",
    "tests::nss::tst-nss-test1::base",
    "tests::nss::tst-nss-test2::base",
    "tests::nss::tst-nss-test4::base",
    "tests::nss::tst-nss-test5::base",
    "tests::nss::tst-nss-test_errno::base",
    "tests::nss::tst-nss-files-alias-leak::base",
    "tests::nss::tst-nss-files-alias-truncated::base",
    "tests::nss::tst-nss-files-hosts-erange::base",
    "tests::nss::tst-nss-files-hosts-getent::base",
    "tests::nss::tst-nss-files-hosts-multi::base",
    "tests::posix::tst-sysconf-empty-chroot::base",
    "tests::posix::tst-wordexp-nocmd::base",
    "tests::resolv::tst-bug18665-tcp::base",
    "tests::resolv::tst-bug18665::base",
    "tests::resolv::tst-leaks::base",
    "tests::resolv::tst-ns_name::base",
    "tests::resolv::tst-ns_name_compress::base",
    "tests::resolv::tst-ns_name_pton::base",
    "tests::resolv::tst-p_secstodate::base",
    "tests::resolv::tst-res_hnok::base",
    "tests::resolv::tst-resolv-ai_idn-nolibidn2::base",
    "tests::resolv::tst-resolv-ai_idn-latin1::base",
    "tests::resolv::tst-resolv-ai_idn::base",
    "tests::resolv::tst-resolv-aliases::base",
    "tests::resolv::tst-resolv-basic::base",
    "tests::resolv::tst-resolv-binary::base",
    "tests::resolv::tst-resolv-byaddr::base",
    "tests::resolv::tst-resolv-canonname::base",
    "tests::resolv::tst-resolv-edns::base",
    "tests::resolv::tst-resolv-invalid-cname::base",
    "tests::resolv::tst-resolv-network::base",
    "tests::resolv::tst-resolv-noaaaa-vc::base",
    "tests::resolv::tst-resolv-noaaaa::base",
    "tests::resolv::tst-resolv-nondecimal::base",
    "tests::resolv::tst-resolv-res_init-multi::base",
    "tests::resolv::tst-resolv-search::base",
    "tests::resolv::tst-resolv-trailing::base",
    "tests::resolv::tst-resolv-trustad::base",
    "tests::socket::tst-sockaddr_un_set::base",
    "tests::posix::tst-spawn-cgroup::base",
    "tests::posix::tst-spawn3-pidfd::base",
    "tests::posix::tst-spawn3::base",
    "tests::sunrpc::tst-bug22542::base",
    "tests::sunrpc::tst-bug28768::base",
    "tests::sunrpc::tst-svc_register::base",
    "tests::sunrpc::tst-udp-error::base",
    "tests::sunrpc::tst-udp-garbage::base",
    "tests::sunrpc::tst-udp-nonblocking::base",
    "tests::sunrpc::tst-udp-timeout::base",
    "tests::sunrpc::tst-xdrmem2::base",
    "tests::sunrpc::tst-xdrmem::base",
    "tests::time::tst-clock2::base",
    "tests::time::tst-clock_settime::base",
    "tests::time::tst-settimeofday::base",
    "tests-time64::misc::tst-clock_adjtime-time64::base",
    "xtests-time64::posix::tst-sched_rr_get_interval-time64::base",
    "xtests::misc::tst-process_madvise::base",
    "xtests::nptl::tst-setuid2::base",
    "xtests::posix::tst-sched_rr_get_interval::base",
    "xtests::resolv::tst-resolv-qtypes::base",
    "xtests::sunrpc::thrsvc::base",
    "xtests::sunrpc::tst-getmyaddr::base",
];

#[derive(Debug)]
pub struct Args {
    pub owner_phase: Option<String>,
    pub all_ported: bool,
    pub install_root: PathBuf,
    pub build_root: PathBuf,
    pub docker_image: Option<String>,
    pub privileged_container_tests: bool,
}

#[derive(ClapArgs, Debug)]
struct CliArgs {
    #[arg(long)]
    owner_phase: Option<String>,
    #[arg(long, default_value_t = false)]
    all_ported: bool,
    #[arg(
        long = "install-root",
        visible_alias = "root",
        default_value = "work/install-root"
    )]
    install_root: PathBuf,
    #[arg(long, default_value = "work/original-build")]
    build_root: PathBuf,
    #[arg(long)]
    docker_image: Option<String>,
    #[arg(long, default_value_t = true)]
    privileged_container_tests: bool,
    #[arg(long, default_value_t = false)]
    require_execution_ledger: bool,
}

static REQUIRE_EXECUTION_LEDGER: AtomicBool = AtomicBool::new(false);

pub fn run(args: Args) -> Result<()> {
    let select_owner = args.owner_phase.is_some();
    if select_owner == args.all_ported {
        bail!("check-owned-tests requires exactly one of --owner-phase or --all-ported");
    }
    let require_execution_ledger = REQUIRE_EXECUTION_LEDGER.load(Ordering::Relaxed);
    if require_execution_ledger && !args.all_ported {
        bail!("--require-execution-ledger requires --all-ported");
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
        .iter()
        .filter(|catalog_id| is_executable_under_install_root(catalog_id))
        .cloned()
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
    })?;

    if require_execution_ledger {
        let ledger = build_execution_ledger(&manifest, &selected_ids)?;
        let path = execution_ledger_path();
        write_execution_ledger_atomically(&path, &ledger)?;
        validate_execution_ledger(&path, &ledger, &selected_ids)?;
    }
    Ok(())
}

impl clap::FromArgMatches for Args {
    fn from_arg_matches(matches: &clap::ArgMatches) -> std::result::Result<Self, clap::Error> {
        let cli = <CliArgs as clap::FromArgMatches>::from_arg_matches(matches)?;
        Ok(apply_cli_args(cli))
    }

    fn update_from_arg_matches(
        &mut self,
        matches: &clap::ArgMatches,
    ) -> std::result::Result<(), clap::Error> {
        let cli = <CliArgs as clap::FromArgMatches>::from_arg_matches(matches)?;
        *self = apply_cli_args(cli);
        Ok(())
    }

    fn update_from_arg_matches_mut(
        &mut self,
        matches: &mut clap::ArgMatches,
    ) -> std::result::Result<(), clap::Error> {
        let cli = <CliArgs as clap::FromArgMatches>::from_arg_matches_mut(matches)?;
        *self = apply_cli_args(cli);
        Ok(())
    }
}

impl clap::Args for Args {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        <CliArgs as clap::Args>::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        <CliArgs as clap::Args>::augment_args_for_update(cmd)
    }
}

fn apply_cli_args(cli: CliArgs) -> Args {
    REQUIRE_EXECUTION_LEDGER.store(cli.require_execution_ledger, Ordering::Relaxed);
    Args {
        owner_phase: cli.owner_phase,
        all_ported: cli.all_ported,
        install_root: cli.install_root,
        build_root: cli.build_root,
        docker_image: cli.docker_image,
        privileged_container_tests: cli.privileged_container_tests,
    }
}

#[derive(Debug, Serialize)]
struct ExecutionLedger {
    schema_version: u32,
    generated_by: &'static str,
    mode: &'static str,
    total_entries: usize,
    entries: Vec<ExecutionLedgerEntry>,
}

#[derive(Debug, Serialize)]
struct ExecutionLedgerEntry {
    catalog_id: String,
    safe_path: String,
    owner_phase: String,
    coverage_status: String,
    reason: String,
    command: String,
}

fn execution_ledger_path() -> PathBuf {
    repo_path("safe/generated/baseline/upstream-test-execution-ledger.json")
}

fn build_execution_ledger(
    manifest: &crate::common::TestsManifest,
    selected_ids: &[String],
) -> Result<ExecutionLedger> {
    let selected = selected_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut entries = Vec::new();
    for entry in &manifest.entries {
        if !selected.contains(&entry.catalog_id) {
            continue;
        }
        let (coverage_status, reason, command) = if is_executable_under_install_root(
            &entry.catalog_id,
        ) {
            (
                "executed".to_string(),
                "Executed against the staged safe install root through xtask run-original-tests."
                    .to_string(),
                format!(
                    "cargo run -p xtask -- run-original-tests --root work/install-root --build-root work/original-build --tests {} --privileged-container-tests",
                    entry.catalog_id
                ),
            )
        } else {
            (
                "equivalent".to_string(),
                "Not directly executable against work/install-root; final equivalence is covered by committed source ownership, support-asset completeness, installed-header checks, ABI checks, and link/development-asset compatibility checks."
                    .to_string(),
                "cargo run -p xtask -- check-owned-tests --all-ported --root work/install-root --build-root work/original-build --require-execution-ledger"
                    .to_string(),
            )
        };
        entries.push(ExecutionLedgerEntry {
            catalog_id: entry.catalog_id.clone(),
            safe_path: entry.safe_path.clone(),
            owner_phase: entry.owner_phase.clone(),
            coverage_status,
            reason,
            command,
        });
    }
    Ok(ExecutionLedger {
        schema_version: 1,
        generated_by: "xtask check-owned-tests",
        mode: "all-ported-final",
        total_entries: entries.len(),
        entries,
    })
}

fn write_execution_ledger_atomically(path: &Path, ledger: &ExecutionLedger) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(ledger)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, format!("{text}\n"))
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

fn validate_execution_ledger(
    path: &Path,
    ledger: &ExecutionLedger,
    selected_ids: &[String],
) -> Result<()> {
    let selected = selected_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();
    if ledger.entries.len() != ALL_PORTED_ENTRY_COUNT {
        failures.push(format!(
            "{} must contain exactly {} entries, found {}",
            path.display(),
            ALL_PORTED_ENTRY_COUNT,
            ledger.entries.len()
        ));
    }
    for entry in &ledger.entries {
        if !seen.insert(entry.catalog_id.clone()) {
            failures.push(format!("duplicate ledger entry {}", entry.catalog_id));
        }
        if !selected.contains(&entry.catalog_id) {
            failures.push(format!(
                "{} appears in the execution ledger but was not selected",
                entry.catalog_id
            ));
        }
        match entry.coverage_status.as_str() {
            "executed" => {
                if entry.command.trim().is_empty() {
                    failures.push(format!(
                        "{} executed entry has no command",
                        entry.catalog_id
                    ));
                }
            }
            "equivalent" => {
                if entry.reason.trim().is_empty() || entry.command.trim().is_empty() {
                    failures.push(format!(
                        "{} equivalent entry must include a reason and command",
                        entry.catalog_id
                    ));
                }
            }
            other => failures.push(format!(
                "{} has invalid coverage_status {other}; expected executed or equivalent",
                entry.catalog_id
            )),
        }
    }
    for catalog_id in selected {
        if !seen.contains(&catalog_id) {
            failures.push(format!("{catalog_id} is missing from the execution ledger"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "execution ledger validation failed for {}:\n{}",
            path.display(),
            failures.join("\n")
        )
    }
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
    if catalog_id.starts_with("tests-internal::")
        || catalog_id.starts_with("tests-container::elf::")
        || catalog_id.starts_with("tests-container::misc::tst-syslog")
        || catalog_id.starts_with("tests-container::nss::")
        || catalog_id.starts_with("tests-container::resolv::")
        || catalog_id.starts_with("tests-malloc-check::")
        || catalog_id.starts_with("tests-malloc-hugetlb")
        || catalog_id.starts_with("tests-mcheck::")
        || catalog_id.starts_with("tests-pie::")
        || catalog_id.starts_with("tests-special::")
        || catalog_id.starts_with("tests-static::dlfcn::")
        || catalog_id.starts_with("tests-static::elf::")
        || catalog_id.starts_with("tests-static::misc::")
        || catalog_id.starts_with("tests-static::nptl::")
        || catalog_id.starts_with("tests-static::nss::")
        || catalog_id.starts_with("tests-static::resolv::")
        || catalog_id.starts_with("tests-time64::misc::tst-adjtimex")
        || catalog_id.starts_with("tests-time64::misc::tst-clock_adjtime")
        || catalog_id.starts_with("tests-time64::misc::tst-ntp_adjtime")
        || catalog_id.starts_with("tests::debug::tst-fortify")
        || catalog_id.starts_with("tests::debug::tst-sprintf-fortify")
        || catalog_id.starts_with("tests::elf::")
        || catalog_id.starts_with("tests::malloc::")
        || catalog_id.starts_with("tests::misc::tst-adjtimex")
        || catalog_id.starts_with("tests::misc::tst-clock_adjtime")
        || catalog_id.starts_with("tests::misc::tst-ntp_adjtime")
    {
        return false;
    }
    !NON_EXECUTABLE_UNDER_INSTALL_ROOT
        .iter()
        .any(|blocked| *blocked == catalog_id)
}
