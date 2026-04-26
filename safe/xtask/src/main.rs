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
        Command::Check04LoaderTests(args) => commands::check_04_loader_tests::run(args),
        Command::Check04LoaderAbi(args) => commands::check_04_loader_abi::run(args),
        Command::Check04LoaderTools(args) => commands::check_04_loader_tools::run(args),
        Command::Check05CoreRuntimeTests(args) => commands::check_05_core_runtime_tests::run(args),
        Command::Check05CoreRuntimeAbi(args) => commands::check_05_core_runtime_abi::run(args),
        Command::Check05RuntimeTools(args) => commands::check_05_runtime_tools::run(args),
        Command::Check05BaseDependentSmoke(args) => {
            commands::check_05_base_dependent_smoke::run(args)
        }
        Command::RunOriginalTests(args) => commands::run_original_tests::run(args),
        Command::CheckAbi(args) => commands::check_abi::run(args),
        Command::InstallRoot(args) => commands::install_root::run(args),
        Command::CheckHeaders(args) => commands::check_headers::run(args),
        Command::LinkCompatSmoke(args) => commands::link_compat_smoke::run(args),
        Command::PackageDeb(args) => commands::package_deb::run(args),
        Command::TestPackageInstall(args) => commands::test_package_install::run(args),
    }
}
