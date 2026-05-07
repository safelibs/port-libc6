use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

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
    super::build::refresh_phase_outputs()?;
    super::link_compat_smoke::run(super::link_compat_smoke::Args {
        install_root: args.install_root,
        build_root: args.build_root,
    })
}
