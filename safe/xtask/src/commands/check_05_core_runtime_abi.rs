use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(ClapArgs, Debug, Default)]
pub struct Args {
    #[arg(long)]
    pub build_root: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    super::check_abi::run(super::check_abi::Args {
        all_dsos: false,
        dso: vec![
            "libc".to_string(),
            "libpthread".to_string(),
            "libthread_db".to_string(),
            "libc_malloc_debug".to_string(),
            "libmemusage".to_string(),
        ],
        build_root: args.build_root,
        strict_symbol_metadata: false,
    })
}
