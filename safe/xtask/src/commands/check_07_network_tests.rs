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
    #[arg(long)]
    pub docker_image: Option<String>,
    #[arg(long, default_value_t = true)]
    pub privileged_container_tests: bool,
}

pub fn run(args: Args) -> Result<()> {
    super::stage_upstream_build::ensure_staged_upstream_build(
        std::path::Path::new("original"),
        &args.build_root,
    )?;
    super::check_owned_tests::run(super::check_owned_tests::Args {
        owner_phase: Some("impl_07_nss_resolver_nscd".to_string()),
        all_ported: false,
        install_root: args.install_root,
        build_root: args.build_root,
        docker_image: args.docker_image,
        privileged_container_tests: args.privileged_container_tests,
        require_execution_ledger: false,
    })
}
