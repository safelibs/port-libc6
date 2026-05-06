use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub build_root: Option<PathBuf>,
    #[arg(
        long = "install-root",
        visible_alias = "root",
        default_value = "work/install-root"
    )]
    pub install_root: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    super::build::run(super::build::Args {
        target: "amd64".to_string(),
        profile: "dev".to_string(),
    })?;
    super::check_abi::run(super::check_abi::Args {
        all_dsos: false,
        dso: vec![
            "libdl".to_string(),
            "libm".to_string(),
            "libmvec".to_string(),
            "libpcprofile".to_string(),
            "librt".to_string(),
            "libutil".to_string(),
        ],
        build_root: args.build_root.clone(),
        strict_symbol_metadata: false,
    })?;
    super::link_compat_smoke::run(super::link_compat_smoke::Args {
        install_root: args.install_root,
        build_root: args
            .build_root
            .unwrap_or_else(|| PathBuf::from("work/original-build")),
        strict_dev_assets: false,
    })
}
