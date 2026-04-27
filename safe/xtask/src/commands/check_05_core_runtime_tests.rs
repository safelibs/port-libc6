use crate::common::{load_tests_manifest, repo_path};
use anyhow::{bail, Result};
use clap::Args as ClapArgs;
use std::collections::BTreeSet;

const OWNER_PHASE: &str = "impl_05_core_runtime_threads_entropy";

const REQUIRED_OUTPUT_DIRS: [&str; 8] = [
    "safe/tests/core",
    "safe/tests/malloc",
    "safe/tests/misc",
    "safe/tests/nptl",
    "safe/tests/nptl_db",
    "safe/tests/setjmp",
    "safe/tests/signal",
    "safe/tests/stdlib",
];

const ALLOWED_STDLIB_TESTS: [&str; 4] = [
    "tst-arc4random-fork",
    "tst-arc4random-stats",
    "tst-arc4random-thread",
    "tst-getrandom",
];

#[derive(ClapArgs, Debug, Default)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let manifest = load_tests_manifest()?;
    let owned = manifest
        .entries
        .iter()
        .filter(|entry| entry.owner_phase == OWNER_PHASE)
        .collect::<Vec<_>>();
    if owned.is_empty() {
        bail!("no phase-5-owned runtime test entries were found");
    }

    let allowed_stdlib = BTreeSet::from(ALLOWED_STDLIB_TESTS.map(str::to_string));
    let mut seen_stdlib = BTreeSet::new();
    let mut failures = Vec::new();

    for path in REQUIRED_OUTPUT_DIRS {
        let path = repo_path(path);
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
        let safe_path = repo_path(&entry.safe_path);
        if !safe_path.exists() {
            failures.push(format!(
                "{} is missing committed safe path {}",
                entry.catalog_id,
                safe_path.display()
            ));
        }
        for support in &entry.support_paths {
            let support_path = repo_path(support);
            if !support_path.exists() {
                failures.push(format!(
                    "{} is missing support path {}",
                    entry.catalog_id,
                    support_path.display()
                ));
            }
        }

        if entry.subdir == "stdlib" {
            let stem = std::path::Path::new(&entry.safe_path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            if !allowed_stdlib.contains(&stem) {
                failures.push(format!(
                    "{} is an unexpected phase-5 stdlib ownership entry",
                    entry.catalog_id
                ));
            } else {
                seen_stdlib.insert(stem);
            }
        }
    }

    if seen_stdlib != allowed_stdlib {
        failures.push(format!(
            "phase-5 stdlib coverage must be limited to {:?}; saw {:?}",
            allowed_stdlib, seen_stdlib
        ));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "phase-5 core runtime test validation failed:\n{}",
            failures.join("\n")
        )
    }
}
