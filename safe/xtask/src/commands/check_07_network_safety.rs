use anyhow::Result;
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug, Default)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    super::audit_safety::run(super::audit_safety::Args {
        verify_policy: true,
        deny_unreviewed_unsafe: true,
        deny_untracked_fallback_c: true,
        deny_shipped_temporary_fallback_binaries: false,
        deny_shipped_private_backend_dsos: false,
        require_cve_disposition: true,
        require_package_scope_clean: true,
    })
}
