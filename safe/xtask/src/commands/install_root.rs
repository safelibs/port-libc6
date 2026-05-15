use crate::common::{
    copy_file_or_symlink, default_upstream_source_build_dir, install_path_to_root,
    load_required_package_entries, make_ld_library_path, repo_path, safe_root, write_pretty_json,
    InstallManifest, InstallManifestMetadata, PackageEntry, PHASE_ID, REQUIRED_PACKAGES,
};
use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use libc_support_tools::{
    backend_assets, fallback_asset_path, find_required_tool, render_wrapper_script,
    tool_binary_name, RequiredTool, RequiredToolKind,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "work/install-root")]
    pub dest: PathBuf,
    #[arg(long, default_value_t = true)]
    pub include_testroot_only: bool,
    #[arg(long, default_value_t = true)]
    pub clean: bool,
}

pub fn run(args: Args) -> Result<()> {
    super::build::refresh_phase_outputs()?;
    let dest = resolve_safe_path(&args.dest);
    materialize_install_root(&dest, args.include_testroot_only, args.clean)
}

pub fn refresh_install_manifests() -> Result<()> {
    let required_entries = load_required_package_entries(false)?;
    let test_entries = load_required_package_entries(true)?;
    write_install_manifests(&required_entries, &test_entries, true)
}

pub fn materialize_install_root(
    dest: &Path,
    include_testroot_only: bool,
    clean: bool,
) -> Result<()> {
    super::build::ensure_active_build_profile("amd64", "release")?;
    if clean && dest.exists() {
        fs::remove_dir_all(dest).with_context(|| format!("failed to remove {}", dest.display()))?;
    }
    fs::create_dir_all(dest).with_context(|| format!("failed to create {}", dest.display()))?;

    let test_entries = load_required_package_entries(include_testroot_only)?;

    for entry in &test_entries {
        materialize_entry(dest, entry)?;
    }
    synthesize_lib64_runtime_mirror(dest)?;

    // Debian ships an empty libpthread_nonshared.a on amd64 for link compatibility.
    let compat_archive = dest.join("usr/lib64/libpthread_nonshared.a");
    if !compat_archive.exists() {
        fs::create_dir_all(
            compat_archive
                .parent()
                .expect("compat archive must have a parent directory"),
        )?;
        crate::common::run_command(
            std::process::Command::new("ar")
                .arg("rcs")
                .arg(&compat_archive),
        )?;
    }

    // Seed the loader cache and a helper env file for consumers.
    let helper_path = safe_root().join("work/install-root.env");
    fs::write(
        &helper_path,
        format!(
            "INSTALL_ROOT={}\nLD_LIBRARY_PATH={}\n",
            dest.display(),
            make_ld_library_path(dest)
        ),
    )
    .with_context(|| format!("failed to write {}", helper_path.display()))?;

    Ok(())
}

fn write_install_manifests(
    required_entries: &[PackageEntry],
    test_entries: &[PackageEntry],
    include_testroot_only: bool,
) -> Result<()> {
    let install_dir = safe_root().join("generated/install-manifests");
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    let required_packages = InstallManifest {
        metadata: InstallManifestMetadata {
            phase: PHASE_ID.to_string(),
            include_testroot_only: false,
            package_count: REQUIRED_PACKAGES.len(),
            packages: REQUIRED_PACKAGES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            notes: vec![
                "Generated directly from the committed package baseline manifests.".to_string(),
                "This manifest is the shipped package promise, excluding testroot-only assets."
                    .to_string(),
            ],
        },
        entries: required_entries.to_vec(),
    };
    let test_install_root = InstallManifest {
        metadata: InstallManifestMetadata {
            phase: PHASE_ID.to_string(),
            include_testroot_only,
            package_count: REQUIRED_PACKAGES.len() + usize::from(include_testroot_only),
            packages: {
                let mut packages = REQUIRED_PACKAGES
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>();
                if include_testroot_only {
                    packages.push("testroot-only".to_string());
                }
                packages
            },
            notes: vec![
                "Generated directly from the committed package baseline manifests.".to_string(),
                "This manifest extends the shipped package promise with testroot_only assets required by the upstream harness.".to_string(),
            ],
        },
        entries: test_entries.to_vec(),
    };

    write_pretty_json(
        &install_dir.join("required-packages.json"),
        &required_packages,
    )?;
    write_pretty_json(
        &install_dir.join("test-install-root.json"),
        &test_install_root,
    )?;
    Ok(())
}

fn materialize_entry(root: &Path, entry: &PackageEntry) -> Result<()> {
    if let Some(tool) = find_required_tool(&entry.path) {
        return materialize_required_tool(root, tool);
    }

    if entry.asset_kind == "generated_compat_archive"
        || entry.asset_kind == "synthetic_empty_archive"
    {
        return materialize_generated_compat_archive(root, entry);
    }
    let out_path = install_path_to_root(root, &entry.path);
    if let Some(target) = &entry.symlink_target {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        if let Ok(metadata) = fs::symlink_metadata(&out_path) {
            if metadata.file_type().is_dir() {
                fs::remove_dir_all(&out_path)?;
            } else {
                fs::remove_file(&out_path)?;
            }
        }
        std::os::unix::fs::symlink(target, &out_path).with_context(|| {
            format!(
                "failed to create symlink {} -> {}",
                out_path.display(),
                target
            )
        })?;
        return Ok(());
    }

    if let Some(source_path) = &entry.source_path {
        if let Some(src) = resolve_source_path(entry, source_path) {
            copy_file_or_symlink(&src, &out_path)?;
        } else if entry.package == "libc6-dbg" || entry.source_origin == "derived_debug" {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::write(&out_path, b"")
                .with_context(|| format!("failed to write {}", out_path.display()))?;
        } else {
            anyhow::bail!(
                "missing source payload for {} from {}",
                entry.path,
                source_path
            );
        }
    } else {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&out_path, b"")
            .with_context(|| format!("failed to write {}", out_path.display()))?;
    }

    Ok(())
}

fn materialize_generated_compat_archive(root: &Path, entry: &PackageEntry) -> Result<()> {
    let out_path = install_path_to_root(root, &entry.path);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    crate::common::run_command(std::process::Command::new("ar").arg("rcs").arg(&out_path))?;
    Ok(())
}

fn materialize_required_tool(root: &Path, tool: &RequiredTool) -> Result<()> {
    let out_path = install_path_to_root(root, tool.entrypoint);
    match tool.kind {
        RequiredToolKind::FallbackWrapper {
            fallback_source_path,
        } => {
            crate::common::touch_executable_text(
                &out_path,
                render_wrapper_script(tool)
                    .as_deref()
                    .expect("fallback wrapper tools must render a wrapper"),
            )?;
            let fallback = install_path_to_root(
                root,
                fallback_asset_path(tool)
                    .as_deref()
                    .expect("fallback wrapper tools must have a fallback asset"),
            );
            copy_file_or_symlink(
                &resolve_workspace_source_path(fallback_source_path)?,
                &fallback,
            )?;
            crate::common::set_executable(&fallback)?;
        }
        RequiredToolKind::RustEntrypoint { .. } => {
            let binary = ensure_required_tool_binary(tool)?;
            copy_file_or_symlink(&binary, &out_path)?;
            crate::common::set_executable(&out_path)?;
            for backend in backend_assets(tool) {
                let path = install_path_to_root(root, backend.install_path);
                copy_file_or_symlink(&resolve_workspace_source_path(backend.source_path)?, &path)?;
                crate::common::set_executable(&path)?;
            }
        }
    }
    Ok(())
}

fn ensure_required_tool_binary(tool: &RequiredTool) -> Result<PathBuf> {
    static BUILT_BINARIES: OnceLock<()> = OnceLock::new();
    if BUILT_BINARIES.get().is_none() {
        crate::common::run_command(
            Command::new("cargo")
                .arg("build")
                .arg("--release")
                .arg("-p")
                .arg("libc-support-tools")
                .arg("--bins")
                .current_dir(safe_root()),
        )?;
        let _ = BUILT_BINARIES.set(());
    }

    let binary_name = tool_binary_name(tool).ok_or_else(|| {
        anyhow::anyhow!("tool {} does not use a Rust entrypoint", tool.entrypoint)
    })?;
    Ok(safe_root().join("target/release").join(binary_name))
}

fn resolve_source_path(entry: &PackageEntry, source_path: &str) -> Option<PathBuf> {
    if let Ok(path) = resolve_workspace_source_path(source_path) {
        return Some(path);
    }

    let staged = default_upstream_source_build_dir()
        .join("testroot.pristine")
        .join(entry.path.trim_start_matches('/'));
    if staged.exists() {
        return Some(staged);
    }

    None
}

fn resolve_workspace_source_path(source_path: &str) -> Result<PathBuf> {
    let direct = repo_path(source_path);
    if direct.exists() {
        return Ok(direct);
    }

    if let Some(stripped) = source_path.strip_prefix("build/") {
        let staged = default_upstream_source_build_dir().join(stripped);
        if staged.exists() {
            return Ok(staged);
        }
    }

    anyhow::bail!("missing source payload {}", source_path)
}

fn resolve_safe_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        safe_root().join(path)
    }
}

fn synthesize_lib64_runtime_mirror(root: &Path) -> Result<()> {
    let usr_lib64 = root.join("usr/lib64");
    if !usr_lib64.exists() {
        return Ok(());
    }
    let lib64 = root.join("lib64");
    fs::create_dir_all(&lib64).with_context(|| format!("failed to create {}", lib64.display()))?;

    for entry in fs::read_dir(&usr_lib64)
        .with_context(|| format!("failed to read {}", usr_lib64.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !should_mirror_to_lib64(&file_name) {
            continue;
        }

        let out_path = lib64.join(file_name.as_ref());
        if let Ok(metadata) = fs::symlink_metadata(&out_path) {
            if metadata.file_type().is_dir() {
                fs::remove_dir_all(&out_path)
                    .with_context(|| format!("failed to remove {}", out_path.display()))?;
            } else {
                fs::remove_file(&out_path)
                    .with_context(|| format!("failed to remove {}", out_path.display()))?;
            }
        }
        std::os::unix::fs::symlink(
            Path::new("../usr/lib64").join(file_name.as_ref()),
            &out_path,
        )
        .with_context(|| format!("failed to create {}", out_path.display()))?;
    }
    Ok(())
}

fn should_mirror_to_lib64(file_name: &str) -> bool {
    file_name == "ld-linux-x86-64.so.2"
        || file_name == "libmemusage.so"
        || file_name == "libpcprofile.so"
        || file_name.contains(".so.")
}
