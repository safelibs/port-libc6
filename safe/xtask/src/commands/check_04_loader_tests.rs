use crate::common::{load_tests_manifest, PHASE_ID};
use anyhow::{bail, Result};
use clap::Args as ClapArgs;

const REQUIRED_OUTPUT_DIRS: [&str; 2] = [
    "safe/tests/sysdeps-x86_64",
    "safe/tests/sysdeps-linux-x86_64",
];

#[derive(ClapArgs, Debug, Default)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let manifest = load_tests_manifest()?;
    let owned = manifest
        .entries
        .iter()
        .filter(|entry| entry.owner_phase == PHASE_ID)
        .collect::<Vec<_>>();
    if owned.is_empty() {
        bail!("no phase-4-owned loader test entries were found");
    }

    let mut failures = Vec::new();
    for path in REQUIRED_OUTPUT_DIRS {
        let path = crate::common::repo_path(path);
        if !path.exists() {
            failures.push(format!(
                "missing required committed output {}",
                path.display()
            ));
        }
    }
    for entry in owned {
        if entry.port_status != "ported" {
            failures.push(format!("{} is not marked ported", entry.catalog_id));
        }
        if has_legacy_phase4_sysdeps_path(&entry.safe_path) {
            failures.push(format!(
                "{} still uses legacy sysdeps path {}",
                entry.catalog_id, entry.safe_path
            ));
        }
        let safe_path = crate::common::repo_path(&entry.safe_path);
        if !safe_path.exists() {
            failures.push(format!(
                "{} is missing committed safe path {}",
                entry.catalog_id,
                safe_path.display()
            ));
        }
        for support in &entry.support_paths {
            if has_legacy_phase4_sysdeps_path(support) {
                failures.push(format!(
                    "{} still references legacy support path {}",
                    entry.catalog_id, support
                ));
            }
            let support_path = crate::common::repo_path(support);
            if !support_path.exists() {
                failures.push(format!(
                    "{} is missing support path {}",
                    entry.catalog_id,
                    support_path.display()
                ));
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "phase-4 loader test validation failed:\n{}",
            failures.join("\n")
        )
    }
}

fn has_legacy_phase4_sysdeps_path(path: &str) -> bool {
    [
        "safe/tests/sysdeps/x86_64/",
        "safe/tests/sysdeps/x86/",
        "safe/tests/sysdeps/unix/sysv/linux/x86_64/",
        "safe/tests/sysdeps/unix/sysv/linux/x86/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}
