use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "ubuntu:24.04")]
    pub docker_image: String,
    #[arg(long, default_value = "work/debs")]
    pub deb_dir: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    super::package_deb::run(super::package_deb::Args {
        out: args.deb_dir.clone(),
        clean: true,
    })?;
    for smoke_set in [
        "basic-required-packages",
        "libc-family-cutover",
        "loader-tools",
        "runtime-tools",
        "network-tools",
        "locale-tools",
    ] {
        super::test_package_install::run(super::test_package_install::Args {
            docker_image: args.docker_image.clone(),
            deb_dir: args.deb_dir.clone(),
            smoke_set: smoke_set.to_string(),
        })?;
    }
    Ok(())
}
