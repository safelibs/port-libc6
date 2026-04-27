use crate::common::{
    copy_file_or_symlink, default_upstream_source_build_dir, ensure_clean_dir, ensure_parent_dir,
    install_path_to_root, load_package_build_manifest, load_package_manifest,
    load_package_manifest_from_path, remove_path_if_exists, repo_path, run_command, safe_root,
    set_executable, touch_executable_text, PackageBuildManifest, PackageBuildSpec, PackageEntry,
    REQUIRED_PACKAGES,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::Args as ClapArgs;
use glob::glob;
use libc_support_tools::{
    backend_assets, fallback_asset_path, find_required_tool, render_wrapper_script,
    tool_binary_name, RequiredTool, RequiredToolKind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "work/debs")]
    pub out: PathBuf,
    #[arg(long, default_value_t = true)]
    pub clean: bool,
}

#[derive(Clone, Debug)]
struct ControlParagraph {
    fields: BTreeMap<String, String>,
}

pub fn run(args: Args) -> Result<()> {
    super::build::run(super::build::Args {
        target: "amd64".to_string(),
        profile: "dev".to_string(),
    })?;
    let build_manifest = load_package_build_manifest()?;
    validate_build_manifest(&build_manifest)?;

    let out_dir = resolve_safe_path(&args.out);
    if args.clean {
        ensure_clean_dir(&out_dir)?;
    } else {
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("failed to create {}", out_dir.display()))?;
    }

    let scratch = safe_root().join("work/package-deb");
    ensure_clean_dir(&scratch)?;

    for spec in &build_manifest.packages {
        stage_package(&build_manifest, spec, &scratch, &out_dir)?;
    }

    Ok(())
}

fn validate_build_manifest(build_manifest: &PackageBuildManifest) -> Result<()> {
    let mut seen = BTreeSet::new();
    for path in &build_manifest.common_files {
        let abs = repo_path(path);
        if !abs.exists() {
            bail!("missing common packaging asset {}", abs.display());
        }
    }
    for package in &build_manifest.packages {
        seen.insert(package.name.as_str());
        for path in package
            .debhelper_files
            .iter()
            .chain(package.local_files.iter())
            .chain(package.helper_files.iter())
        {
            let abs = repo_path(path);
            if !abs.exists() {
                bail!("missing packaging asset {}", abs.display());
            }
            if path.ends_with(".install") {
                validate_install_file(&abs)?;
            }
        }
        let package_manifest = repo_path(&package.package_manifest);
        if !package_manifest.exists() {
            bail!("missing package manifest {}", package_manifest.display());
        }
    }

    let expected: BTreeSet<&str> = REQUIRED_PACKAGES.iter().copied().collect();
    if seen != expected {
        bail!(
            "package-build-manifest must describe exactly {:?}, found {:?}",
            expected,
            seen
        );
    }

    let control_path = repo_path(&build_manifest.control);
    let paragraphs = parse_control_file(&control_path)?;
    let mut package_stanzas = BTreeSet::new();
    for paragraph in paragraphs {
        if let Some(package) = paragraph.fields.get("Package") {
            package_stanzas.insert(package.clone());
        }
    }
    if !package_stanzas.is_superset(&expected.iter().map(|value| value.to_string()).collect()) {
        bail!("safe/debian/control is missing one or more required package stanzas");
    }
    Ok(())
}

fn stage_package(
    build_manifest: &PackageBuildManifest,
    spec: &PackageBuildSpec,
    scratch_root: &Path,
    out_dir: &Path,
) -> Result<()> {
    let package_root = scratch_root.join(&spec.name);
    ensure_clean_dir(&package_root)?;
    fs::create_dir_all(package_root.join("DEBIAN"))
        .with_context(|| format!("failed to create {}", package_root.display()))?;

    let package_manifest = load_package_manifest_from_path(&spec.package_manifest)?;
    let control = binary_control_paragraph(repo_path(&build_manifest.control), &spec.name)?;

    for file in &spec.debhelper_files {
        if file.ends_with(".dirs") {
            apply_dirs_file(&package_root, &repo_path(file))?;
        } else if file.ends_with(".links") {
            apply_links_file(&package_root, &repo_path(file))?;
        } else if file.ends_with(".README.Debian") {
            stage_doc_file(&package_root, &spec.name, &repo_path(file), "README.Debian")?;
        } else if file.ends_with(".NEWS") {
            stage_doc_file(&package_root, &spec.name, &repo_path(file), "NEWS")?;
        } else if file.ends_with(".manpages") {
            stage_manpages(&package_root, &repo_path(file))?;
        } else if file.ends_with(".lintian-overrides") {
            stage_lintian_override(&package_root, &spec.name, &repo_path(file))?;
        } else if file.ends_with(".symbols.amd64") {
            stage_doc_file(
                &package_root,
                &spec.name,
                &repo_path(file),
                "libc6.symbols.amd64",
            )?;
        }
    }

    for helper in &spec.helper_files {
        stage_helper_file(&package_root, &spec.name, &repo_path(helper))?;
    }

    for entry in package_manifest.entries {
        stage_package_entry(&package_root, &entry)?;
    }

    if spec.name == "libc6" || spec.name == "libc6-dev" {
        stage_multiarch_compat(&package_root)?;
    }

    stage_common_docs(
        &package_root,
        &spec.name,
        &repo_path(&build_manifest.copyright),
    )?;
    stage_debian_metadata(
        &package_root,
        &control,
        &build_manifest.safe_package_version,
        spec,
    )?;

    let out_path = out_dir.join(format!(
        "{}_{}_{}.deb",
        spec.name, build_manifest.safe_package_version, spec.architecture
    ));
    run_command(
        Command::new("dpkg-deb")
            .arg("--build")
            .arg("--root-owner-group")
            .arg(&package_root)
            .arg(&out_path),
    )
    .with_context(|| format!("failed to build {}", out_path.display()))?;
    Ok(())
}

fn stage_package_entry(package_root: &Path, entry: &PackageEntry) -> Result<()> {
    if entry.shipped_status != "shipped" {
        return Ok(());
    }
    if entry.package == "libc6-dbg" {
        return stage_debug_entry(package_root, entry);
    }
    if entry.asset_kind == "generated_compat_archive" {
        return stage_generated_compat_archive(package_root, entry);
    }
    if let Some(tool) = find_required_tool(&entry.path) {
        return stage_required_tool(package_root, tool);
    }
    if let Some(target) = &entry.symlink_target {
        let out_path = install_path_to_root(package_root, &entry.path);
        ensure_parent_dir(&out_path)?;
        remove_path_if_exists(&out_path)?;
        symlink(target, &out_path)
            .with_context(|| format!("failed to create {}", out_path.display()))?;
        return Ok(());
    }
    let source_path = entry
        .source_path
        .as_deref()
        .ok_or_else(|| anyhow!("missing source_path for {}", entry.path))?;
    let source = resolve_payload_source(entry, source_path)?;
    let out_path = install_path_to_root(package_root, &entry.path);
    copy_file_or_symlink(&source, &out_path)?;
    Ok(())
}

fn stage_generated_compat_archive(package_root: &Path, entry: &PackageEntry) -> Result<()> {
    let out_path = install_path_to_root(package_root, &entry.path);
    ensure_parent_dir(&out_path)?;
    run_command(Command::new("ar").arg("rcs").arg(&out_path))?;
    Ok(())
}

fn stage_required_tool(package_root: &Path, tool: &RequiredTool) -> Result<()> {
    let wrapper_path = install_path_to_root(package_root, tool.entrypoint);
    match tool.kind {
        RequiredToolKind::FallbackWrapper {
            fallback_source_path,
        } => {
            touch_executable_text(
                &wrapper_path,
                render_wrapper_script(tool)
                    .as_deref()
                    .expect("fallback wrapper tools must render a wrapper"),
            )?;

            let fallback_path = install_path_to_root(
                package_root,
                fallback_asset_path(tool)
                    .as_deref()
                    .expect("fallback wrapper tools must have a fallback asset path"),
            );
            let source = resolve_workspace_source_path(fallback_source_path)?;
            copy_file_or_symlink(&source, &fallback_path)?;
            if fs::symlink_metadata(&fallback_path)?.file_type().is_file() {
                set_executable(&fallback_path)?;
            }
        }
        RequiredToolKind::RustEntrypoint { .. } => {
            let binary = ensure_required_tool_binary(tool)?;
            copy_file_or_symlink(&binary, &wrapper_path)?;
            set_executable(&wrapper_path)?;
            for backend in backend_assets(tool) {
                let backend_path = install_path_to_root(package_root, backend.install_path);
                let source = resolve_workspace_source_path(backend.source_path)?;
                copy_file_or_symlink(&source, &backend_path)?;
                if fs::symlink_metadata(&backend_path)?.file_type().is_file() {
                    set_executable(&backend_path)?;
                }
            }
        }
    }
    Ok(())
}

fn ensure_required_tool_binary(tool: &RequiredTool) -> Result<PathBuf> {
    static BUILT_BINARIES: OnceLock<()> = OnceLock::new();
    if BUILT_BINARIES.get().is_none() {
        run_command(
            Command::new("cargo")
                .arg("build")
                .arg("--release")
                .arg("-p")
                .arg("libc-support-tools")
                .arg("--bins")
                .current_dir(safe_root()),
        )
        .with_context(|| format!("failed to build Rust tool binary for {}", tool.entrypoint))?;
        let _ = BUILT_BINARIES.set(());
    }

    let binary_name = tool_binary_name(tool)
        .ok_or_else(|| anyhow!("tool {} does not use a Rust entrypoint", tool.entrypoint))?;
    let binary = safe_root().join("target/release").join(binary_name);
    if !binary.exists() {
        bail!("missing built Rust tool binary {}", binary.display());
    }
    Ok(binary)
}

fn stage_debug_entry(package_root: &Path, entry: &PackageEntry) -> Result<()> {
    let source_install_path = entry
        .source_path
        .as_deref()
        .ok_or_else(|| anyhow!("libc6-dbg entry {} is missing source_path", entry.path))?;
    let source = resolve_debug_source(source_install_path)?;
    let out_path = install_path_to_root(package_root, &entry.path);
    ensure_parent_dir(&out_path)?;
    run_command(
        Command::new("objcopy")
            .arg("--only-keep-debug")
            .arg(&source)
            .arg(&out_path),
    )
    .with_context(|| format!("failed to derive {}", out_path.display()))?;
    Ok(())
}

fn resolve_debug_source(source_install_path: &str) -> Result<PathBuf> {
    let direct = default_upstream_source_build_dir()
        .join("testroot.pristine")
        .join(source_install_path.trim_start_matches('/'));
    if direct.exists() {
        return Ok(direct);
    }

    for package in REQUIRED_PACKAGES
        .iter()
        .copied()
        .filter(|package| *package != "libc6-dbg")
    {
        let manifest = load_package_manifest(package)?;
        if let Some(entry) = manifest
            .entries
            .into_iter()
            .find(|entry| entry.path == source_install_path)
        {
            if let Some(source_path) = entry.source_path.as_deref() {
                let resolved = resolve_payload_source(&entry, source_path)?;
                if resolved.exists() {
                    return Ok(resolved);
                }
            }
        }
    }

    bail!("missing debug source payload for installed path {source_install_path}")
}

fn stage_debian_metadata(
    package_root: &Path,
    control: &ControlParagraph,
    safe_version: &str,
    spec: &PackageBuildSpec,
) -> Result<()> {
    let debian_root = package_root.join("DEBIAN");
    let control_text = render_binary_control(control, safe_version, &spec.architecture);
    fs::write(debian_root.join("control"), control_text)
        .with_context(|| format!("failed to write {}", debian_root.join("control").display()))?;

    let Some(script_prefix) = package_script_prefix(&spec.name) else {
        return Ok(());
    };
    for suffix in [
        "preinst",
        "postinst",
        "postrm",
        "prerm",
        "config",
        "templates",
        "triggers",
    ] {
        let source = repo_path(format!("safe/debian/{script_prefix}.{suffix}"));
        if !source.exists() {
            continue;
        }
        let dest = debian_root.join(suffix);
        copy_file_or_symlink(&source, &dest)?;
        if matches!(
            suffix,
            "preinst" | "postinst" | "postrm" | "prerm" | "config"
        ) {
            set_executable(&dest)?;
        }
    }
    Ok(())
}

fn stage_common_docs(package_root: &Path, package: &str, copyright: &Path) -> Result<()> {
    stage_doc_file(package_root, package, copyright, "copyright")
}

fn stage_doc_file(package_root: &Path, package: &str, source: &Path, name: &str) -> Result<()> {
    let dest = package_root.join("usr/share/doc").join(package).join(name);
    copy_file_or_symlink(source, &dest)
}

fn stage_lintian_override(package_root: &Path, package: &str, source: &Path) -> Result<()> {
    let dest = package_root
        .join("usr/share/lintian/overrides")
        .join(package);
    copy_file_or_symlink(source, &dest)
}

fn stage_helper_file(package_root: &Path, package: &str, source: &Path) -> Result<()> {
    let name = source
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("invalid helper filename {}", source.display()))?;
    match name {
        "nscd.service" => {
            let dest = package_root.join("usr/lib/systemd/system/nscd.service");
            copy_file_or_symlink(source, &dest)?;
        }
        "nscd.tmpfiles" => {
            let dest = package_root.join("usr/lib/tmpfiles.d/nscd.conf");
            copy_file_or_symlink(source, &dest)?;
        }
        "nsscheck.sh" => {
            let dest = package_root
                .join("usr/share/doc")
                .join(package)
                .join("nsscheck.sh");
            copy_file_or_symlink(source, &dest)?;
        }
        "nscd.init" => {
            let dest = package_root.join("etc/init.d/nscd");
            copy_file_or_symlink(source, &dest)?;
            set_executable(&dest)?;
        }
        _ => {}
    }
    Ok(())
}

fn apply_dirs_file(package_root: &Path, dirs_file: &Path) -> Result<()> {
    for line in fs::read_to_string(dirs_file)
        .with_context(|| format!("failed to read {}", dirs_file.display()))?
        .lines()
    {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        fs::create_dir_all(package_root.join(trimmed)).with_context(|| {
            format!("failed to create {}", package_root.join(trimmed).display())
        })?;
    }
    Ok(())
}

fn apply_links_file(package_root: &Path, links_file: &Path) -> Result<()> {
    for line in fs::read_to_string(links_file)
        .with_context(|| format!("failed to read {}", links_file.display()))?
        .lines()
    {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let target = parts
            .next()
            .ok_or_else(|| anyhow!("invalid links entry in {}", links_file.display()))?;
        let link = parts
            .next()
            .ok_or_else(|| anyhow!("invalid links entry in {}", links_file.display()))?;
        let link_path = package_root.join(link);
        ensure_parent_dir(&link_path)?;
        remove_path_if_exists(&link_path)?;
        let target_path = Path::new("/").join(target);
        let relative = relative_path(
            link_path.parent().expect("link must have a parent"),
            &package_root.join(target_path.strip_prefix("/").unwrap()),
        );
        symlink(&relative, &link_path)
            .with_context(|| format!("failed to create {}", link_path.display()))?;
    }
    Ok(())
}

fn stage_manpages(package_root: &Path, manpages_file: &Path) -> Result<()> {
    for line in fs::read_to_string(manpages_file)
        .with_context(|| format!("failed to read {}", manpages_file.display()))?
        .lines()
    {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let source = resolve_safe_debian_path(trimmed);
        let (dest, executable) = manpage_destination(package_root, &source)?;
        copy_file_or_symlink(&source, &dest)?;
        if executable {
            set_executable(&dest)?;
        }
    }
    Ok(())
}

fn manpage_destination(package_root: &Path, source: &Path) -> Result<(PathBuf, bool)> {
    let rel = source
        .strip_prefix(repo_path("safe/debian/local/manpages"))
        .with_context(|| format!("unexpected manpage source {}", source.display()))?;
    let components = rel.components().collect::<Vec<_>>();
    let filename = source
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("invalid manpage filename {}", source.display()))?;

    if components.len() >= 2 {
        let lang = components[0].as_os_str().to_string_lossy();
        let section = filename
            .rsplit('.')
            .next()
            .ok_or_else(|| anyhow!("invalid manpage filename {}", source.display()))?;
        let stem = filename
            .strip_suffix(&format!(".{section}"))
            .unwrap_or(filename)
            .split('.')
            .next()
            .unwrap_or(filename);
        Ok((
            package_root
                .join("usr/share/man")
                .join(lang.as_ref())
                .join(format!("man{section}"))
                .join(format!("{stem}.{section}")),
            false,
        ))
    } else {
        let section = filename
            .rsplit('.')
            .next()
            .ok_or_else(|| anyhow!("invalid manpage filename {}", source.display()))?;
        Ok((
            package_root
                .join("usr/share/man")
                .join(format!("man{section}"))
                .join(filename),
            false,
        ))
    }
}

fn stage_multiarch_compat(package_root: &Path) -> Result<()> {
    let usr_lib64 = package_root.join("usr/lib64");
    if usr_lib64.exists() {
        mirror_tree_as_symlinks(
            &usr_lib64,
            &package_root.join("usr/lib/x86_64-linux-gnu"),
            "usr/lib64",
        )?;
    }

    let include_link = package_root.join("usr/include/x86_64-linux-gnu");
    if !include_link.exists() {
        ensure_parent_dir(&include_link)?;
        symlink(".", &include_link)
            .with_context(|| format!("failed to create {}", include_link.display()))?;
    }
    Ok(())
}

fn mirror_tree_as_symlinks(src_root: &Path, dst_root: &Path, src_label: &str) -> Result<()> {
    for entry in walkdir::WalkDir::new(src_root) {
        let entry = entry.with_context(|| format!("failed to walk {}", src_root.display()))?;
        let rel = entry
            .path()
            .strip_prefix(src_root)
            .with_context(|| format!("failed to strip prefix {}", src_root.display()))?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = dst_root.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)
                .with_context(|| format!("failed to create {}", dest.display()))?;
            continue;
        }
        ensure_parent_dir(&dest)?;
        remove_path_if_exists(&dest)?;
        let target = relative_path(
            dest.parent().expect("mirror destination must have parent"),
            entry.path(),
        );
        symlink(&target, &dest).with_context(|| {
            format!(
                "failed to mirror {} into {}",
                entry.path().display(),
                src_label
            )
        })?;
    }
    Ok(())
}

fn relative_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    let from_components = from_dir
        .components()
        .filter_map(normal_component)
        .collect::<Vec<_>>();
    let to_components = to_path
        .components()
        .filter_map(normal_component)
        .collect::<Vec<_>>();

    let mut common = 0usize;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut result = PathBuf::new();
    for _ in common..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common..] {
        result.push(component);
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    result
}

fn normal_component(component: Component<'_>) -> Option<String> {
    match component {
        Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn render_binary_control(
    paragraph: &ControlParagraph,
    safe_version: &str,
    architecture_override: &str,
) -> String {
    let mut lines = Vec::new();
    push_field(
        &mut lines,
        "Package",
        paragraph.fields.get("Package").unwrap(),
    );
    push_field(&mut lines, "Version", safe_version);
    push_field(&mut lines, "Architecture", architecture_override);
    for key in [
        "Section",
        "Priority",
        "Essential",
        "Multi-Arch",
        "Pre-Depends",
        "Depends",
        "Recommends",
        "Suggests",
        "Provides",
        "Breaks",
        "Replaces",
    ] {
        if let Some(value) = paragraph.fields.get(key) {
            push_field(
                &mut lines,
                key,
                &value.replace("@SAFE_VERSION@", safe_version),
            );
        }
    }
    push_field(
        &mut lines,
        "Maintainer",
        paragraph
            .fields
            .get("Maintainer")
            .map(String::as_str)
            .unwrap_or("SafeLibs Port Authors <ports@example.invalid>"),
    );
    push_field(
        &mut lines,
        "Description",
        paragraph
            .fields
            .get("Description")
            .map(String::as_str)
            .unwrap_or("SafeLibs placeholder package"),
    );
    format!("{}\n", lines.join("\n"))
}

fn push_field(lines: &mut Vec<String>, key: &str, value: &str) {
    let mut value_lines = value.lines();
    if let Some(first) = value_lines.next() {
        lines.push(format!("{key}: {first}"));
        for line in value_lines {
            lines.push(format!(" {line}"));
        }
    }
}

fn binary_control_paragraph(path: PathBuf, package: &str) -> Result<ControlParagraph> {
    let paragraphs = parse_control_file(&path)?;
    paragraphs
        .into_iter()
        .find(|paragraph| paragraph.fields.get("Package").map(String::as_str) == Some(package))
        .ok_or_else(|| anyhow!("missing package stanza {package} in {}", path.display()))
}

fn parse_control_file(path: &Path) -> Result<Vec<ControlParagraph>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut paragraphs = Vec::new();
    let mut current = BTreeMap::<String, String>::new();
    let mut current_key = String::new();

    for raw_line in text.lines() {
        if raw_line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(ControlParagraph { fields: current });
                current = BTreeMap::new();
                current_key.clear();
            }
            continue;
        }
        if raw_line.starts_with(' ') {
            let value = current
                .get_mut(&current_key)
                .ok_or_else(|| anyhow!("continuation line without key in {}", path.display()))?;
            value.push('\n');
            value.push_str(raw_line.trim_start());
            continue;
        }
        let (key, value) = raw_line
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid control line {raw_line}"))?;
        current_key = key.to_string();
        current.insert(key.to_string(), value.trim_start().to_string());
    }
    if !current.is_empty() {
        paragraphs.push(ControlParagraph { fields: current });
    }
    Ok(paragraphs)
}

fn package_script_prefix(package: &str) -> Option<&'static str> {
    match package {
        "libc6" => Some("libc"),
        "libc-bin" => Some("libc-bin"),
        "locales" => Some("locales"),
        "nscd" => Some("nscd"),
        "libc6-dev" | "libc-dev-bin" | "libc6-dbg" => None,
        _ => None,
    }
}

fn validate_install_file(path: &Path) -> Result<()> {
    for line in fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .lines()
    {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let source = trimmed
            .split_whitespace()
            .next()
            .expect("split_whitespace must yield a token");
        if !matches_install_source_needing_validation(source) {
            continue;
        }
        if source.contains('$') || source.contains('*') {
            validate_glob_source(source)?;
            continue;
        }
        if resolve_install_source(source).is_none() {
            bail!(
                "unable to resolve install entry source {source} referenced by {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn matches_install_source_needing_validation(source: &str) -> bool {
    source.starts_with("debian/") || source.starts_with("nscd/")
}

fn validate_glob_source(pattern: &str) -> Result<()> {
    let pattern = if pattern.starts_with("debian/") {
        repo_path(format!("safe/{pattern}"))
    } else if pattern.starts_with("nscd/") {
        repo_path(format!("original/{pattern}"))
    } else {
        default_upstream_source_build_dir()
            .join("testroot.pristine")
            .join(pattern)
    };
    let mut matched = false;
    for entry in glob(&pattern.display().to_string())
        .with_context(|| format!("invalid glob {}", pattern.display()))?
    {
        if entry.is_ok() {
            matched = true;
            break;
        }
    }
    if !matched {
        bail!(
            "glob {} did not match any committed inputs",
            pattern.display()
        );
    }
    Ok(())
}

fn resolve_install_source(source: &str) -> Option<PathBuf> {
    let candidates = [
        repo_path(format!("safe/{source}")),
        repo_path(format!("original/{source}")),
        default_upstream_source_build_dir()
            .join("testroot.pristine")
            .join(source),
        repo_path(source),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn resolve_payload_source(entry: &PackageEntry, source_path: &str) -> Result<PathBuf> {
    if let Some(stripped) = source_path.strip_prefix("original/debian/") {
        let committed = repo_path(format!("safe/debian/{stripped}"));
        if committed.exists() {
            return Ok(committed);
        }
    }

    if let Ok(path) = resolve_workspace_source_path(source_path) {
        return Ok(path);
    }

    let staged = default_upstream_source_build_dir()
        .join("testroot.pristine")
        .join(entry.path.trim_start_matches('/'));
    if staged.exists() {
        return Ok(staged);
    }

    bail!(
        "missing payload source {} for installed path {}",
        source_path,
        entry.path
    )
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

    bail!("missing source payload {}", source_path)
}

fn resolve_safe_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        safe_root().join(path)
    }
}

fn resolve_safe_debian_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("debian/") {
        repo_path(format!("safe/debian/{stripped}"))
    } else {
        repo_path(path)
    }
}
