use crate::common::{
    command_output, make_ld_library_path, resolve_safe_workspace_path,
    resolve_upstream_source_build_dir, safe_root,
};
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

#[derive(Clone, Copy)]
struct SmokeCase {
    name: &'static str,
    source: &'static str,
    extra_args: &'static [&'static str],
    execute: bool,
}

const SMOKE_CASES: [SmokeCase; 4] = [
    SmokeCase {
        name: "startup-objects",
        source: "int main(void) { return 0; }\n",
        extra_args: &[
            "-nostartfiles",
            "-Wl,--entry=main",
            "-Wl,--dynamic-linker=/usr/lib64/ld-linux-x86-64.so.2",
        ],
        execute: false,
    },
    SmokeCase {
        name: "ordinary-objects",
        source: "#include <dlfcn.h>\n#include <pthread.h>\nint main(void) { return dlopen(0, RTLD_NOW) == 0; }\n",
        extra_args: &["-ldl", "-lpthread", "-lutil", "-lanl", "-lresolv"],
        execute: true,
    },
    SmokeCase {
        name: "static-link",
        source: "int main(void) { return 0; }\n",
        extra_args: &["-static"],
        execute: true,
    },
    SmokeCase {
        name: "glibc-private",
        source: "extern void *_dl_find_dso_for_object(void *);\n__asm__(\".symver _dl_find_dso_for_object,_dl_find_dso_for_object@GLIBC_PRIVATE\");\nint main(void) { return _dl_find_dso_for_object((void*)main) == 0; }\n",
        extra_args: &[],
        execute: true,
    },
];

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let install_root = resolve_safe_workspace_path(&args.install_root)?;
    let build_root = resolve_upstream_source_build_dir(&args.build_root)?;
    super::install_root::materialize_install_root(&install_root, true, false)?;

    let scratch = safe_root().join("work/link-smoke");
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .with_context(|| format!("failed to remove {}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch)
        .with_context(|| format!("failed to create {}", scratch.display()))?;

    for case in SMOKE_CASES {
        run_case(&build_root, &install_root, &scratch, case)?;
    }
    Ok(())
}

fn run_case(
    build_root: &PathBuf,
    install_root: &PathBuf,
    scratch: &PathBuf,
    case: SmokeCase,
) -> Result<()> {
    let source = scratch.join(format!("{}.c", case.name));
    let binary = scratch.join(case.name);
    fs::write(&source, case.source)
        .with_context(|| format!("failed to write {}", source.display()))?;

    let libdir = install_root.join("usr/lib64");
    let mut command = Command::new("gcc");
    command
        .arg(format!("--sysroot={}", install_root.display()))
        .arg("-B")
        .arg(&libdir)
        .arg("-I")
        .arg(install_root.join("usr/include"))
        .arg("-L")
        .arg(&libdir)
        .arg("-Wl,-rpath-link")
        .arg(&libdir)
        .arg("-Wl,-dynamic-linker,/usr/lib64/ld-linux-x86-64.so.2")
        .arg("-o")
        .arg(&binary)
        .arg(&source);
    for extra in case.extra_args {
        command.arg(extra);
    }
    let status = command.status().with_context(|| "failed to spawn gcc")?;
    if !status.success() {
        bail!("link smoke case {} failed to link", case.name);
    }
    if !case.execute {
        return Ok(());
    }

    let mut run = if case.name == "static-link" {
        Command::new(&binary)
    } else {
        let mut cmd = Command::new(build_root.join("elf/ld-linux-x86-64.so.2"));
        cmd.arg("--library-path")
            .arg(make_ld_library_path(install_root))
            .arg(&binary);
        cmd
    };
    run.env("GCONV_PATH", build_root.join("iconvdata"))
        .env("LOCPATH", build_root.join("localedata"))
        .env("LC_ALL", "C");
    let output = command_output(&mut run)?;
    if output.contains("error:") {
        bail!("link smoke case {} emitted failure text", case.name);
    }
    Ok(())
}
