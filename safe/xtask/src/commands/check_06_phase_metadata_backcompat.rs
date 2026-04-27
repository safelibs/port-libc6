use crate::common::{command_output, safe_root};
use anyhow::{bail, Result};
use clap::Args as ClapArgs;
use std::process::Command;

#[derive(ClapArgs, Debug, Default)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    let help = command_output(
        Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg("xtask")
            .arg("--")
            .arg("--help")
            .current_dir(safe_root()),
    )?;
    for command in [
        "check_04_loader_tests",
        "check_04_loader_abi",
        "check_04_loader_tools",
        "check_05_core_runtime_tests",
        "check_05_core_runtime_abi",
        "check_05_runtime_tools",
        "check_05_base_dependent_smoke",
    ] {
        if !help.contains(command) {
            bail!("xtask --help is missing legacy verifier command {command}");
        }
    }

    super::build::run(super::build::Args {
        target: "amd64".to_string(),
        profile: "dev".to_string(),
    })?;
    super::check_04_loader_tests::run(Default::default())?;
    super::check_04_loader_abi::run(Default::default())?;
    super::check_04_loader_tools::run(Default::default())?;
    super::check_05_core_runtime_tests::run(Default::default())?;
    super::check_05_core_runtime_abi::run(Default::default())?;
    super::check_05_runtime_tools::run(Default::default())?;
    super::check_05_base_dependent_smoke::run(super::check_05_base_dependent_smoke::Args {
        install_root: "work/install-root".into(),
        build_root: "work/original-build".into(),
    })?;
    Ok(())
}
