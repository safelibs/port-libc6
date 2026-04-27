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
            "ld.so".to_string(),
            "libc".to_string(),
            "libpthread".to_string(),
            "libthread_db".to_string(),
            "libc_malloc_debug".to_string(),
            "libmemusage".to_string(),
        ],
        build_root: args.build_root.clone(),
    })?;
    super::link_compat_smoke::run(super::link_compat_smoke::Args {
        install_root: args.install_root.clone(),
        build_root: args
            .build_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("work/original-build")),
    })?;
    super::check_headers::run(super::check_headers::Args {
        install_root: args.install_root,
        lang: Vec::new(),
    })
}
