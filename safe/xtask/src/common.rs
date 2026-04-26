use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use toml::Value as TomlValue;
use walkdir::WalkDir;

pub const PHASE_ID: &str = "impl_05_core_runtime_threads_entropy";
pub const COMPLETED_PHASES: [&str; 4] = [
    "impl_02_hybrid_abi_shell",
    "impl_03_packaging_and_harness",
    "impl_04_loader_startup_secure_exec",
    "impl_05_core_runtime_threads_entropy",
];
pub const REQUIRED_PACKAGES: [&str; 7] = [
    "libc6",
    "libc-bin",
    "libc6-dev",
    "libc-dev-bin",
    "locales",
    "nscd",
    "libc6-dbg",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AbiBaseline {
    pub dso_id: String,
    pub primary_oracle: String,
    #[serde(default)]
    pub auxiliary_oracles: Vec<String>,
    #[serde(default)]
    pub installed_paths: Vec<String>,
    pub build_id: Option<String>,
    pub soname: Option<String>,
    #[serde(default)]
    pub needed: Vec<String>,
    #[serde(default)]
    pub exported_symbols: Vec<String>,
    #[serde(default)]
    pub map_files: Vec<AbiMapFile>,
    #[serde(default)]
    pub symlist_files: Vec<AbiSymlistFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AbiMapFile {
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub versions: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AbiSymlistFile {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageManifest {
    #[serde(flatten)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub entries: Vec<PackageEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageEntry {
    pub package: String,
    pub path: String,
    pub source_path: Option<String>,
    pub source_origin: String,
    pub scope: String,
    pub shipped_status: String,
    pub asset_kind: String,
    pub executable: bool,
    pub symlink_target: Option<String>,
    pub owner_phase: Option<String>,
    pub verification: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestCatalog {
    pub metadata: serde_json::Value,
    pub entries: Vec<TestCatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestCatalogEntry {
    pub catalog_id: String,
    pub subdir: String,
    pub name: String,
    pub family: String,
    pub origin_selector: String,
    pub variant: String,
    pub has_checked_in_baseline_result: bool,
    pub requires_container_or_privileged_execution: bool,
    #[serde(default)]
    pub origin_makefiles: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestPortPlan {
    pub metadata: serde_json::Value,
    pub entries: Vec<TestPortPlanEntry>,
    pub support_subtree: SupportSubtree,
    #[serde(default)]
    pub zero_entry_subdirs: Vec<ZeroEntrySubdir>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestPortPlanEntry {
    pub catalog_id: String,
    pub owner_phase: String,
    pub destination_path: String,
    pub source_path: Option<String>,
    #[serde(default)]
    pub companion_assets: Vec<String>,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SupportSubtree {
    pub owner_phase: String,
    pub source_root: String,
    pub destination_root: String,
    pub asset_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ZeroEntrySubdir {
    pub subdir: String,
    pub owner_phase: String,
    pub status: String,
    pub destination_root: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FallbackInventory {
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub entries: Vec<FallbackInventoryEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FallbackInventoryEntry {
    pub path: String,
    pub source_path: Option<String>,
    pub classification: String,
    pub owning_phase: String,
    pub shipped: bool,
    #[serde(default)]
    pub package_scope_refs: Vec<String>,
    pub audit_notes: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestsManifest {
    pub metadata: TestsManifestMetadata,
    pub entries: Vec<TestsManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestsManifestMetadata {
    pub phase: String,
    #[serde(default)]
    pub notes: Vec<String>,
    pub source_plan: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestsManifestEntry {
    pub catalog_id: String,
    pub safe_path: String,
    #[serde(default)]
    pub support_paths: Vec<String>,
    pub owner_phase: String,
    pub port_status: String,
    pub source_path: Option<String>,
    pub subdir: String,
    pub family: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallManifest {
    pub metadata: InstallManifestMetadata,
    pub entries: Vec<PackageEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallManifestMetadata {
    pub phase: String,
    pub include_testroot_only: bool,
    pub package_count: usize,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageBuildManifest {
    pub metadata: serde_json::Value,
    pub safe_package_version: String,
    pub control: String,
    pub copyright: String,
    #[serde(default)]
    pub common_files: Vec<String>,
    #[serde(default)]
    pub packages: Vec<PackageBuildSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageBuildSpec {
    pub name: String,
    pub architecture: String,
    pub package_manifest: String,
    #[serde(default)]
    pub debhelper_files: Vec<String>,
    #[serde(default)]
    pub local_files: Vec<String>,
    #[serde(default)]
    pub helper_files: Vec<String>,
}

pub fn safe_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest must live under safe/")
        .to_path_buf()
}

pub fn repo_root() -> PathBuf {
    safe_root()
        .parent()
        .expect("safe/ must live under repo root")
        .to_path_buf()
}

pub fn repo_path(path: impl AsRef<str>) -> PathBuf {
    let path = PathBuf::from(path.as_ref());
    if path.is_absolute() {
        path
    } else {
        repo_root().join(path)
    }
}

pub fn package_build_manifest_path() -> PathBuf {
    safe_root().join("generated/packaging/package-build-manifest.json")
}

pub fn upstream_build_dir() -> PathBuf {
    safe_root().join("upstream-tests/build")
}

pub fn default_upstream_source_build_dir() -> PathBuf {
    safe_root().join("work/original-build")
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("expected absolute path, got {}", path.display());
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    bail!("path escapes filesystem root: {}", path.display());
                }
            }
        }
    }
    Ok(normalized)
}

pub fn resolve_safe_workspace_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        safe_root().join(path)
    };
    normalize_absolute_path(&absolute)
}

fn migrate_legacy_repo_build_dir(target: &Path) -> Result<()> {
    let legacy = repo_root().join("build");
    if !legacy.exists() {
        return Ok(());
    }
    if target == legacy {
        bail!(
            "repo-root build scratch is forbidden; move it under {} or {} instead",
            safe_root().join("work").display(),
            upstream_build_dir().display()
        );
    }

    if target.exists() {
        if !target.is_dir() {
            bail!(
                "upstream build root is not a directory: {}",
                target.display()
            );
        }
        fs::remove_dir_all(&legacy)
            .with_context(|| format!("failed to remove legacy scratch {}", legacy.display()))?;
        return Ok(());
    }

    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", target.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    if let Err(rename_error) = fs::rename(&legacy, target) {
        copy_tree(&legacy, target).with_context(|| {
            format!(
                "failed to relocate legacy scratch {} to {} after rename error: {rename_error}",
                legacy.display(),
                target.display()
            )
        })?;
        fs::remove_dir_all(&legacy)
            .with_context(|| format!("failed to remove legacy scratch {}", legacy.display()))?;
    }
    Ok(())
}

fn replace_expected_sequences(
    contents: &mut [u8],
    from: &[u8],
    to: &[u8],
    label: &str,
    expected_matches: usize,
) -> Result<usize> {
    if from.len() != to.len() {
        bail!("replacement for {label} changed instruction width");
    }

    let offsets = contents
        .windows(from.len())
        .enumerate()
        .filter_map(|(index, window)| (window == from).then_some(index))
        .collect::<Vec<_>>();
    match offsets.as_slice() {
        [] => Ok(0),
        _ if offsets.len() != expected_matches => bail!(
            "refusing to patch staged libc {}: found {} copies of {}",
            default_upstream_source_build_dir()
                .join("libc.so")
                .display(),
            offsets.len(),
            label
        ),
        _ => {
            for offset in offsets {
                contents[offset..(offset + from.len())].copy_from_slice(to);
            }
            Ok(expected_matches)
        }
    }
}

fn replace_c_string(
    contents: &mut [u8],
    from: &str,
    to: &str,
    label: &str,
    expected_matches: usize,
) -> Result<usize> {
    if to.len() > from.len() {
        bail!("replacement for {label} exceeds reserved string space");
    }

    let mut from_bytes = from.as_bytes().to_vec();
    from_bytes.push(0);
    let offsets = contents
        .windows(from_bytes.len())
        .enumerate()
        .filter_map(|(index, window)| (window == from_bytes.as_slice()).then_some(index))
        .collect::<Vec<_>>();
    match offsets.as_slice() {
        [] => Ok(0),
        _ if offsets.len() != expected_matches => bail!(
            "refusing to patch staged helper {}: found {} copies of {}",
            default_upstream_source_build_dir()
                .join("support/test-container")
                .display(),
            offsets.len(),
            label
        ),
        _ => {
            for offset in offsets {
                let slot = &mut contents[offset..(offset + from_bytes.len())];
                slot.fill(0);
                slot[..to.len()].copy_from_slice(to.as_bytes());
            }
            Ok(expected_matches)
        }
    }
}

fn repair_staged_libc_signal_syscalls(build_root: &Path) -> Result<()> {
    let libc_path = build_root.join("libc.so");
    if !libc_path.exists() {
        return Ok(());
    }

    let mut contents =
        fs::read(&libc_path).with_context(|| format!("failed to read {}", libc_path.display()))?;
    let patched_rt_sigaction = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x00, 0x02, 0x00, 0x40],
        &[0xb8, 0x0d, 0x00, 0x00, 0x00],
        "__libc_sigaction rt_sigaction syscall",
        1,
    )?;
    let patched_rt_sigreturn = replace_expected_sequences(
        &mut contents,
        &[0x48, 0xc7, 0xc0, 0x01, 0x02, 0x00, 0x40],
        &[0x48, 0xc7, 0xc0, 0x0f, 0x00, 0x00, 0x00],
        "__restore_rt rt_sigreturn syscall",
        1,
    )?;
    let patched_kill = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x3e, 0x00, 0x00, 0x40],
        &[0xb8, 0x3e, 0x00, 0x00, 0x00],
        "kill syscall",
        1,
    )?;
    let patched_rt_sigpending = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x0a, 0x02, 0x00, 0x40],
        &[0xb8, 0x0a, 0x02, 0x00, 0x00],
        "sigpending rt_sigpending syscall",
        1,
    )?;
    let patched_rt_sigsuspend = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x82, 0x00, 0x00, 0x40],
        &[0xb8, 0x82, 0x00, 0x00, 0x00],
        "sigsuspend rt_sigsuspend syscall",
        2,
    )?;
    let patched_rt_sigaltstack = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x0d, 0x02, 0x00, 0x40],
        &[0xb8, 0x0d, 0x02, 0x00, 0x00],
        "sigaltstack rt_sigaltstack syscall",
        1,
    )?;
    let patched_rt_sigtimedwait = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x0b, 0x02, 0x00, 0x40],
        &[0xb8, 0x0b, 0x02, 0x00, 0x00],
        "sigtimedwait rt_sigtimedwait syscall",
        2,
    )?;
    let patched_rt_sigqueueinfo = replace_expected_sequences(
        &mut contents,
        &[0xb8, 0x0c, 0x02, 0x00, 0x40],
        &[0xb8, 0x0c, 0x02, 0x00, 0x00],
        "sigqueue rt_sigqueueinfo syscall",
        1,
    )?;

    if patched_rt_sigaction == 0
        && patched_rt_sigreturn == 0
        && patched_kill == 0
        && patched_rt_sigpending == 0
        && patched_rt_sigsuspend == 0
        && patched_rt_sigaltstack == 0
        && patched_rt_sigtimedwait == 0
        && patched_rt_sigqueueinfo == 0
    {
        return Ok(());
    }

    fs::write(&libc_path, contents)
        .with_context(|| format!("failed to patch {}", libc_path.display()))?;
    Ok(())
}

fn staged_build_alias_root() -> PathBuf {
    PathBuf::from("/tmp/port-libc6-build")
}

fn ensure_staged_build_alias(build_root: &Path) -> Result<PathBuf> {
    let alias = staged_build_alias_root();
    match fs::symlink_metadata(&alias) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = fs::read_link(&alias)
                .with_context(|| format!("failed to read symlink {}", alias.display()))?;
            if target == build_root {
                return Ok(alias);
            }
            fs::remove_file(&alias)
                .with_context(|| format!("failed to remove stale alias {}", alias.display()))?;
        }
        Ok(_) => {
            bail!(
                "refusing to reuse non-symlink staged build alias {}",
                alias.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect alias {}", alias.display()));
        }
    }
    symlink(build_root, &alias).with_context(|| {
        format!(
            "failed to create staged build alias {} -> {}",
            alias.display(),
            build_root.display()
        )
    })?;
    Ok(alias)
}

fn repair_staged_test_container_helper(build_root: &Path) -> Result<Option<PathBuf>> {
    let helper_path = build_root.join("support/test-container");
    if !helper_path.exists() {
        return Ok(None);
    }

    let alias_root = ensure_staged_build_alias(build_root)?;
    let alias_root_text = alias_root.display().to_string();
    let alias_loader_text = format!("{alias_root_text}/elf/ld-linux-x86-64.so.2");
    let mut contents = fs::read(&helper_path)
        .with_context(|| format!("failed to read {}", helper_path.display()))?;
    let mut patched_root = 0;
    let mut patched_loader = 0;
    for root in [repo_root().join("build"), build_root.to_path_buf()] {
        let root_text = root.display().to_string();
        let loader_text = format!("{root_text}/elf/ld-linux-x86-64.so.2");
        patched_root += replace_c_string(
            &mut contents,
            &root_text,
            &alias_root_text,
            "support_objdir_root",
            1,
        )?;
        patched_loader += replace_c_string(
            &mut contents,
            &loader_text,
            &alias_loader_text,
            "support_objdir_elf_ldso",
            1,
        )?;
    }

    if patched_root == 0 && patched_loader == 0 {
        return Ok(Some(alias_root));
    }

    fs::write(&helper_path, contents)
        .with_context(|| format!("failed to patch {}", helper_path.display()))?;
    Ok(Some(alias_root))
}

fn shell_library_path_expr(root_expr: &str) -> String {
    [
        root_expr.to_string(),
        format!("{root_expr}/math"),
        format!("{root_expr}/elf"),
        format!("{root_expr}/dlfcn"),
        format!("{root_expr}/nss"),
        format!("{root_expr}/nis"),
        format!("{root_expr}/rt"),
        format!("{root_expr}/resolv"),
        format!("{root_expr}/mathvec"),
        format!("{root_expr}/support"),
        format!("{root_expr}/nptl"),
    ]
    .join(":")
}

fn build_container_exec_line(container_root: &Path) -> String {
    let root_expr = format!("\"{}\"", container_root.display());
    let library_path = shell_library_path_expr(&root_expr);
    format!(
        "    exec env GCONV_PATH={root_expr}/iconvdata LOCPATH={root_expr}/localedata LC_ALL=C  {root_expr}/elf/ld-linux-x86-64.so.2 --library-path {library_path} {root_expr}/support/test-container env GCONV_PATH={root_expr}/iconvdata LOCPATH={root_expr}/localedata LC_ALL=C  {root_expr}/elf/ld-linux-x86-64.so.2 --library-path {library_path} ${{1+\"$@\"}}"
    )
}

fn repair_staged_testrun_script(build_root: &Path, container_root: Option<&Path>) -> Result<()> {
    let testrun_path = build_root.join("testrun.sh");
    if !testrun_path.exists() {
        return Ok(());
    }

    let original = fs::read_to_string(&testrun_path)
        .with_context(|| format!("failed to read {}", testrun_path.display()))?;
    let builddir_expr = "\"${builddir}\"";
    let mut patched = original.clone();
    for root in [repo_root().join("build"), build_root.to_path_buf()] {
        let root_text = root.display().to_string();
        if patched.contains(&root_text) {
            patched = patched.replace(&root_text, builddir_expr);
        }
    }

    if let Some(container_root) = container_root {
        let mut lines = Vec::new();
        let mut in_container_branch = false;
        let mut replaced_container_exec = false;
        for line in patched.lines() {
            let trimmed = line.trim();
            if trimmed == "container)" {
                in_container_branch = true;
                lines.push(line.to_string());
                continue;
            }
            if in_container_branch && line.trim_start().starts_with("exec ") {
                lines.push(build_container_exec_line(container_root));
                replaced_container_exec = true;
                continue;
            }
            if in_container_branch && trimmed == ";;" {
                in_container_branch = false;
            }
            lines.push(line.to_string());
        }
        if replaced_container_exec {
            patched = lines.join("\n");
            if original.ends_with('\n') {
                patched.push('\n');
            }
        }
    }

    if patched == original {
        return Ok(());
    }

    fs::write(&testrun_path, patched)
        .with_context(|| format!("failed to patch {}", testrun_path.display()))?;
    Ok(())
}

pub fn resolve_upstream_source_build_dir(path: &Path) -> Result<PathBuf> {
    let resolved = resolve_safe_workspace_path(path)?;
    let work_root = normalize_absolute_path(&safe_root().join("work"))?;
    let harness_root = upstream_build_dir();
    let legacy_root = normalize_absolute_path(&repo_root().join("build"))?;

    if resolved == harness_root {
        bail!(
            "{} is reserved for run-original-tests scratch; use {} for the staged upstream build tree",
            harness_root.display(),
            default_upstream_source_build_dir().display()
        );
    }
    if resolved.starts_with(&legacy_root) {
        bail!(
            "repo-root build scratch is forbidden for phase verification: {}",
            resolved.display()
        );
    }
    if !resolved.starts_with(&work_root) {
        bail!(
            "upstream source build roots must live under {}: {}",
            work_root.display(),
            resolved.display()
        );
    }

    migrate_legacy_repo_build_dir(&resolved)?;
    if !resolved.exists() {
        bail!(
            "staged upstream build tree is missing: {}",
            resolved.display()
        );
    }
    repair_staged_libc_signal_syscalls(&resolved)?;
    let container_root = repair_staged_test_container_helper(&resolved)?;
    repair_staged_testrun_script(&resolved, container_root.as_deref())?;
    Ok(resolved)
}

pub fn write_pretty_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    ensure_parent_dir(path)?;
    let text = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{text}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    ensure_parent_dir(path)?;
    let text = toml::to_string_pretty(value)?;
    fs::write(path, format!("{text}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    Ok(())
}

pub fn remove_dir_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

pub fn copy_file_or_symlink(src: &Path, dst: &Path) -> Result<()> {
    ensure_parent_dir(dst)?;
    if let Ok(metadata) = fs::symlink_metadata(dst) {
        if metadata.file_type().is_dir() {
            bail!("refusing to overwrite directory {}", dst.display());
        }
        match fs::remove_file(dst) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to remove {}", dst.display()));
            }
        }
    }

    let metadata =
        fs::symlink_metadata(src).with_context(|| format!("failed to stat {}", src.display()))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(src)
            .with_context(|| format!("failed to read symlink {}", src.display()))?;
        symlink(&target, dst).with_context(|| {
            format!(
                "failed to create symlink {} -> {}",
                dst.display(),
                target.display()
            )
        })?;
    } else {
        fs::copy(src, dst)
            .with_context(|| format!("failed to copy {} to {}", src.display(), dst.display()))?;
        let perms = metadata.permissions();
        fs::set_permissions(dst, perms)
            .with_context(|| format!("failed to set permissions on {}", dst.display()))?;
    }
    Ok(())
}

pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    for entry in WalkDir::new(src) {
        let entry = entry.with_context(|| format!("failed to walk {}", src.display()))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .with_context(|| format!("failed to strip prefix {}", src.display()))?;
        let out = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&out)
                .with_context(|| format!("failed to create {}", out.display()))?;
        } else {
            copy_file_or_symlink(entry.path(), &out)?;
        }
    }
    Ok(())
}

pub fn run_command(command: &mut Command) -> Result<()> {
    let debug = format!("{command:?}");
    let status = command
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to spawn {debug}"))?;
    if !status.success() {
        bail!("command failed ({status}): {debug}");
    }
    Ok(())
}

pub fn command_output(command: &mut Command) -> Result<String> {
    let debug = format!("{command:?}");
    let output = command
        .output()
        .with_context(|| format!("failed to spawn {debug}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("command failed ({}): {debug}\n{stderr}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn load_toml(path: &Path) -> Result<TomlValue> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn write_toml_value(path: &Path, value: &TomlValue) -> Result<()> {
    ensure_parent_dir(path)?;
    let text = toml::to_string_pretty(value)?;
    fs::write(path, format!("{text}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn abi_baselines() -> Result<Vec<AbiBaseline>> {
    let mut baselines: Vec<AbiBaseline> = Vec::new();
    for entry in fs::read_dir(safe_root().join("generated/baseline/abi"))
        .with_context(|| "failed to read ABI baseline directory")?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        baselines.push(load_json(&path)?);
    }
    baselines.sort_by(|left, right| left.dso_id.cmp(&right.dso_id));
    Ok(baselines)
}

pub fn load_package_manifest(package: &str) -> Result<PackageManifest> {
    load_json(&safe_root().join(format!("generated/baseline/package-files/{package}.json")))
}

pub fn load_package_manifest_from_path(path: &str) -> Result<PackageManifest> {
    load_json(&repo_path(path))
}

pub fn load_required_package_entries(include_testroot_only: bool) -> Result<Vec<PackageEntry>> {
    let mut entries = Vec::new();
    for package in REQUIRED_PACKAGES {
        entries.extend(load_package_manifest(package)?.entries);
    }
    if include_testroot_only {
        entries.extend(load_package_manifest("testroot-only")?.entries);
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries.dedup_by(|left, right| left.path == right.path);
    Ok(entries)
}

pub fn load_test_catalog() -> Result<TestCatalog> {
    load_json(&safe_root().join("generated/baseline/test-catalog.json"))
}

pub fn load_test_port_plan() -> Result<TestPortPlan> {
    load_json(&safe_root().join("generated/baseline/test-port-plan.json"))
}

pub fn load_tests_manifest() -> Result<TestsManifest> {
    let path = safe_root().join("tests/manifest.toml");
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn load_package_build_manifest() -> Result<PackageBuildManifest> {
    load_json(&package_build_manifest_path())
}

pub fn touch_executable_text(path: &Path, contents: &str) -> Result<()> {
    ensure_parent_dir(path)?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to chmod {}", path.display()))?;
    Ok(())
}

pub fn set_metadata_phase(doc: &mut TomlValue, phase: &str) -> Result<()> {
    let metadata = doc
        .get_mut("metadata")
        .and_then(TomlValue::as_table_mut)
        .ok_or_else(|| anyhow!("missing [metadata] table"))?;
    metadata.insert("phase".to_string(), TomlValue::String(phase.to_string()));
    Ok(())
}

pub fn upsert_fallback_entry(
    entries: &mut Vec<FallbackInventoryEntry>,
    new_entry: FallbackInventoryEntry,
) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|entry| entry.path == new_entry.path)
    {
        *existing = new_entry;
    } else {
        entries.push(new_entry);
        entries.sort_by(|left, right| left.path.cmp(&right.path));
    }
}

pub fn version_name_cmp(left: &str, right: &str) -> Ordering {
    match (left == "GLIBC_PRIVATE", right == "GLIBC_PRIVATE") {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let parse = |value: &str| {
                value
                    .strip_prefix("GLIBC_")
                    .unwrap_or(value)
                    .split('.')
                    .map(|part| part.parse::<u32>().unwrap_or(0))
                    .collect::<Vec<_>>()
            };
            parse(left).cmp(&parse(right))
        }
    }
}

pub fn install_path_to_root(root: &Path, install_path: &str) -> PathBuf {
    let rel = install_path.trim_start_matches('/');
    root.join(rel)
}

pub fn repo_relative_path(path: &Path) -> Result<String> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let rel = path
        .strip_prefix(repo_root())
        .with_context(|| format!("path is not under repo root: {}", path.display()))?;
    Ok(rel.display().to_string())
}

pub fn remove_path_if_exists(path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_dir() {
            fs::remove_dir_all(path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        } else {
            fs::remove_file(path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    Ok(())
}

pub fn ensure_clean_dir(path: &Path) -> Result<()> {
    remove_dir_if_exists(path)?;
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(())
}

pub fn set_executable(path: &Path) -> Result<()> {
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to chmod {}", path.display()))?;
    Ok(())
}

pub fn make_ld_library_path(root: &Path) -> String {
    [root.join("usr/lib64"), root.join("lib64")]
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}
