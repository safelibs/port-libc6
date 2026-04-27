mod commands;
mod common;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Workspace task runner for the safe libc6 port")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    IngestBaseline(commands::ingest_baseline::Args),
    AuditSafety(commands::audit_safety::Args),
    Build(commands::build::Args),
    StageUpstreamBuild(commands::stage_upstream_build::Args),
    CheckOwnedTests(commands::check_owned_tests::Args),
    #[command(name = "check_04_loader_tests")]
    Check04LoaderTests(commands::check_04_loader_tests::Args),
    #[command(name = "check_04_loader_abi")]
    Check04LoaderAbi(commands::check_04_loader_abi::Args),
    #[command(name = "check_04_loader_tools")]
    Check04LoaderTools(commands::check_04_loader_tools::Args),
    #[command(name = "check_05_core_runtime_tests")]
    Check05CoreRuntimeTests(commands::check_05_core_runtime_tests::Args),
    #[command(name = "check_05_core_runtime_abi")]
    Check05CoreRuntimeAbi(commands::check_05_core_runtime_abi::Args),
    #[command(name = "check_05_runtime_tools")]
    Check05RuntimeTools(commands::check_05_runtime_tools::Args),
    #[command(name = "check_05_base_dependent_smoke")]
    Check05BaseDependentSmoke(commands::check_05_base_dependent_smoke::Args),
    #[command(name = "check_06_phase_metadata_backcompat")]
    Check06PhaseMetadataBackcompat(commands::check_06_phase_metadata_backcompat::Args),
    #[command(name = "check_06_io_stdio_tests")]
    Check06IoStdioTests(commands::check_06_io_stdio_tests::Args),
    #[command(name = "check_06_io_stdio_abi")]
    Check06IoStdioAbi(commands::check_06_io_stdio_abi::Args),
    #[command(name = "check_06_io_stdio_packages")]
    Check06IoStdioPackages(commands::check_06_io_stdio_packages::Args),
    #[command(name = "check_06_io_stdio_safety")]
    Check06IoStdioSafety(commands::check_06_io_stdio_safety::Args),
    #[command(name = "check_07_network_tests")]
    Check07NetworkTests(commands::check_07_network_tests::Args),
    #[command(name = "check_07_network_abi")]
    Check07NetworkAbi(commands::check_07_network_abi::Args),
    #[command(name = "check_07_network_packages")]
    Check07NetworkPackages(commands::check_07_network_packages::Args),
    #[command(name = "check_07_network_safety")]
    Check07NetworkSafety(commands::check_07_network_safety::Args),
    #[command(name = "check_08_locale_tests")]
    Check08LocaleTests(commands::check_08_locale_tests::Args),
    #[command(name = "check_08_locale_abi")]
    Check08LocaleAbi(commands::check_08_locale_abi::Args),
    #[command(name = "check_08_locale_packages")]
    Check08LocalePackages(commands::check_08_locale_packages::Args),
    #[command(name = "check_08_locale_safety")]
    Check08LocaleSafety(commands::check_08_locale_safety::Args),
    RunOriginalTests(commands::run_original_tests::Args),
    CheckAbi(commands::check_abi::Args),
    InstallRoot(commands::install_root::Args),
    CheckHeaders(commands::check_headers::Args),
    LinkCompatSmoke(commands::link_compat_smoke::Args),
    PackageDeb(commands::package_deb::Args),
    TestPackageInstall(commands::test_package_install::Args),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::IngestBaseline(args) => commands::ingest_baseline::run(args),
        Command::AuditSafety(args) => commands::audit_safety::run(args),
        Command::Build(args) => commands::build::run(args),
        Command::StageUpstreamBuild(args) => commands::stage_upstream_build::run(args),
        Command::CheckOwnedTests(args) => commands::check_owned_tests::run(args),
        Command::Check04LoaderTests(args) => commands::check_04_loader_tests::run(args),
        Command::Check04LoaderAbi(args) => commands::check_04_loader_abi::run(args),
        Command::Check04LoaderTools(args) => commands::check_04_loader_tools::run(args),
        Command::Check05CoreRuntimeTests(args) => commands::check_05_core_runtime_tests::run(args),
        Command::Check05CoreRuntimeAbi(args) => commands::check_05_core_runtime_abi::run(args),
        Command::Check05RuntimeTools(args) => commands::check_05_runtime_tools::run(args),
        Command::Check05BaseDependentSmoke(args) => {
            commands::check_05_base_dependent_smoke::run(args)
        }
        Command::Check06PhaseMetadataBackcompat(args) => {
            commands::check_06_phase_metadata_backcompat::run(args)
        }
        Command::Check06IoStdioTests(args) => commands::check_06_io_stdio_tests::run(args),
        Command::Check06IoStdioAbi(args) => commands::check_06_io_stdio_abi::run(args),
        Command::Check06IoStdioPackages(args) => commands::check_06_io_stdio_packages::run(args),
        Command::Check06IoStdioSafety(args) => commands::check_06_io_stdio_safety::run(args),
        Command::Check07NetworkTests(args) => commands::check_07_network_tests::run(args),
        Command::Check07NetworkAbi(args) => commands::check_07_network_abi::run(args),
        Command::Check07NetworkPackages(args) => commands::check_07_network_packages::run(args),
        Command::Check07NetworkSafety(args) => commands::check_07_network_safety::run(args),
        Command::Check08LocaleTests(args) => commands::check_08_locale_tests::run(args),
        Command::Check08LocaleAbi(args) => commands::check_08_locale_abi::run(args),
        Command::Check08LocalePackages(args) => commands::check_08_locale_packages::run(args),
        Command::Check08LocaleSafety(args) => commands::check_08_locale_safety::run(args),
        Command::RunOriginalTests(args) => commands::run_original_tests::run(args),
        Command::CheckAbi(args) => commands::check_abi::run(args),
        Command::InstallRoot(args) => commands::install_root::run(args),
        Command::CheckHeaders(args) => commands::check_headers::run(args),
        Command::LinkCompatSmoke(args) => commands::link_compat_smoke::run(args),
        Command::PackageDeb(args) => commands::package_deb::run(args),
        Command::TestPackageInstall(args) => commands::test_package_install::run(args),
    }
}
