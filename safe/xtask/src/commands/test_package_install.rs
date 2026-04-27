use crate::common::{repo_relative_path, repo_root, safe_package_version, safe_root};
use anyhow::{bail, Context, Result};
use clap::Args as ClapArgs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long, default_value = "ubuntu:24.04")]
    pub docker_image: String,
    #[arg(long, default_value = "work/debs")]
    pub deb_dir: PathBuf,
    #[arg(long, default_value = "basic-required-packages")]
    pub smoke_set: String,
}

pub fn run(args: Args) -> Result<()> {
    if args.smoke_set != "basic-required-packages"
        && args.smoke_set != "libc-family-cutover"
        && args.smoke_set != "loader-tools"
        && args.smoke_set != "runtime-tools"
        && args.smoke_set != "network-tools"
        && args.smoke_set != "locale-tools"
    {
        bail!(
            "unsupported smoke set {}; expected basic-required-packages, libc-family-cutover, loader-tools, runtime-tools, network-tools, or locale-tools",
            args.smoke_set
        );
    }

    let deb_dir = resolve_safe_path(&args.deb_dir);
    if !deb_dir.exists() {
        bail!("missing deb directory {}", deb_dir.display());
    }
    let deb_dir_rel = repo_relative_path(&deb_dir)?;

    let script = r#"set -euo pipefail
deb_dir_rel=$SAFE_DEB_DIR_REL
safe_version=$SAFE_VERSION

log() {
  printf '\n==> %s\n' "$*"
}

verify_safe_provenance() {
  local pkg version arch policy
  for pkg in libc6 libc6-dev libc-dev-bin libc-bin libc6-dbg locales nscd; do
    version=$(dpkg-query -W -f='${Version}' "$pkg")
    arch=$(dpkg-query -W -f='${Architecture}' "$pkg")
    if [ "$version" != "$safe_version" ]; then
      printf 'unexpected version for %s: %s (expected %s)\n' "$pkg" "$version" "$safe_version" >&2
      return 1
    fi
    policy=$(apt-cache policy "$pkg")
    printf '%s\n' "$policy"
    if ! printf '%s\n' "$policy" | grep -Fq "file:/tmp/safelibs-apt-repo"; then
      printf 'apt-cache policy for %s does not reference the local safe repo\n' "$pkg" >&2
      return 1
    fi
    printf 'selected %s %s (%s)\n' "$pkg" "$version" "$arch"
  done
}

manifest_entry_field() {
  local manifest=$1
  local path=$2
  local field=$3
  jq -r --arg path "$path" --arg field "$field" \
    '.entries[] | select(.path == $path) | .[$field] // empty' "$manifest"
}

compare_public_payload() {
  local manifest=$1
  local path=$2
  local source_path backend_path installed_target source_target
  source_path=$(manifest_entry_field "$manifest" "$path" source_path)
  if [ -z "$source_path" ]; then
    printf 'manifest %s is missing source_path for %s\n' "$manifest" "$path" >&2
    return 1
  fi
  if [ -L "$path" ]; then
    installed_target=$(readlink -f "$path")
  else
    installed_target=$path
  fi
  if [ -L "$source_path" ]; then
    source_target=$(readlink -f "$source_path")
  else
    source_target=$source_path
  fi
  if [ ! -f "$installed_target" ] || [ ! -f "$source_target" ]; then
    printf 'missing staged payload comparison target for %s\n' "$path" >&2
    return 1
  fi
  if [ "$(sha256sum "$installed_target" | awk '{print $1}')" != "$(sha256sum "$source_target" | awk '{print $1}')" ]; then
    printf 'installed payload for %s does not match staged source %s\n' "$path" "$source_path" >&2
    return 1
  fi
  backend_path="/usr/libexec/safelibs/backends/$(basename "$path")"
  if [ -f "$backend_path" ] && [ "$(sha256sum "$installed_target" | awk '{print $1}')" = "$(sha256sum "$backend_path" | awk '{print $1}')" ]; then
    printf 'installed public payload %s still matches private backend %s\n' "$path" "$backend_path" >&2
    return 1
  fi
}

smoke_basic_required_packages() {
  log "Verifying required package payloads from the committed install manifest"
  jq -r '.entries[] | select(.shipped_status == "shipped") | [.package, .path] | @tsv' \
    /workspace/safe/generated/install-manifests/required-packages.json | \
  while IFS=$'\t' read -r pkg path; do
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
      printf 'missing installed payload for %s: %s\n' "$pkg" "$path" >&2
      exit 1
    fi
  done

  log "Checking basic required-package entrypoints"
  for path in \
    /usr/bin/gencat \
    /usr/bin/getconf \
    /usr/bin/getent \
    /usr/bin/iconv \
    /usr/bin/ld.so \
    /usr/bin/ldd \
    /usr/bin/locale \
    /usr/bin/localedef \
    /usr/bin/pldd \
    /usr/bin/tzselect \
    /usr/bin/zdump \
    /usr/sbin/iconvconfig \
    /usr/sbin/ldconfig \
    /usr/sbin/zic \
    /usr/sbin/locale-gen \
    /usr/sbin/update-locale \
    /usr/sbin/validlocale \
    /usr/share/locales/install-language-pack \
    /usr/share/locales/remove-language-pack \
    /usr/sbin/nscd; do
    if [ ! -e "$path" ]; then
      printf 'missing entrypoint %s\n' "$path" >&2
      exit 1
    fi
  done

  log "Checking detached debug companions"
  jq -r '.entries[] | [.path, .source_path] | @tsv' \
    /workspace/safe/generated/baseline/package-files/libc6-dbg.json | \
  while IFS=$'\t' read -r debug_path source_path; do
    local build_id expected
    if [ ! -f "$debug_path" ]; then
      printf 'missing debug companion %s\n' "$debug_path" >&2
      exit 1
    fi
    build_id=$(readelf -n "$source_path" | awk '/Build ID:/ { print $3; exit }')
    expected="/usr/lib/debug/.build-id/${build_id:0:2}/${build_id:2}.debug"
    if [ "$expected" != "$debug_path" ]; then
      printf 'debug manifest mismatch for %s: expected %s got %s\n' \
        "$source_path" "$expected" "$debug_path" >&2
      exit 1
    fi
  done

  if [ ! -e /etc/nsswitch.conf ]; then
    printf 'libc-bin postinst did not leave /etc/nsswitch.conf in place\n' >&2
    exit 1
  fi
  if [ ! -e /etc/default/locale ]; then
    printf 'locales postinst did not create /etc/default/locale\n' >&2
    exit 1
  fi
}

smoke_loader_tools() {
  log "Verifying loader-tool payloads from the committed install manifest"
  jq -r '.entries[] | select(.verification == "loader-tools" and .shipped_status == "shipped") | [.package, .path] | @tsv' \
    /workspace/safe/generated/install-manifests/required-packages.json | \
  while IFS=$'\t' read -r pkg path; do
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
      printf 'missing installed loader-tool payload for %s: %s\n' "$pkg" "$path" >&2
      exit 1
    fi
  done

  log "Checking loader-tool entrypoints"
  test -x /usr/bin/ld.so
  test -x /usr/bin/ldd
  test -x /usr/sbin/ldconfig
  test -x /usr/libexec/safelibs/loader-tools/ld.so.backend
  test -x /usr/libexec/safelibs/loader-tools/ldconfig.backend

  /usr/bin/ld.so --help >/tmp/ld-so-help.txt
  /usr/bin/ldd --version >/tmp/ldd-version.txt
  /usr/bin/ldd /bin/true >/tmp/ldd-output.txt
  /usr/sbin/ldconfig -p >/tmp/ldconfig-cache.txt

  if [ ! -s /tmp/ldd-version.txt ] || [ ! -s /tmp/ldd-output.txt ]; then
    printf 'ldd smoke output was empty\n' >&2
    exit 1
  fi
  if ! grep -q 'libs found in cache' /tmp/ldconfig-cache.txt; then
    printf 'ldconfig smoke output did not include cache summary\n' >&2
    exit 1
  fi
}

smoke_runtime_tools() {
  log "Verifying runtime-tool payloads from the committed install manifest"
  jq -r '.entries[] | select(.verification == "runtime-tools" and .shipped_status == "shipped") | [.package, .path] | @tsv' \
    /workspace/safe/generated/install-manifests/required-packages.json | \
  while IFS=$'\t' read -r pkg path; do
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
      printf 'missing installed runtime-tool payload for %s: %s\n' "$pkg" "$path" >&2
      exit 1
    fi
  done

  log "Checking runtime-tool entrypoints"
  test -x /usr/bin/pldd
  test -x /usr/libexec/safelibs/runtime-tools/pldd.backend
  if [ -e /usr/lib/pt_chown ]; then
    printf '/usr/lib/pt_chown should remain absent on amd64\n' >&2
    exit 1
  fi
}

smoke_network_tools() {
  log "Verifying network-tool and network-DSO payloads from the committed install manifest"
  jq -r '.entries[] | select(.verification == "network-tools" and .shipped_status == "shipped") | [.package, .path] | @tsv' \
    /workspace/safe/generated/install-manifests/required-packages.json | \
  while IFS=$'\t' read -r pkg path; do
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
      printf 'missing installed network payload for %s: %s\n' "$pkg" "$path" >&2
      exit 1
    fi
  done

  log "Checking network-facing DSO cutover provenance"
  for manifest in \
    /workspace/safe/generated/baseline/package-files/libc6.json \
    /workspace/safe/generated/install-manifests/required-packages.json; do
    for path in \
      /usr/lib64/libanl.so.1 \
      /usr/lib64/libnsl.so.1 \
      /usr/lib64/libnss_compat.so.2 \
      /usr/lib64/libnss_dns.so.2 \
      /usr/lib64/libnss_files.so.2 \
      /usr/lib64/libnss_hesiod.so.2 \
      /usr/lib64/libresolv.so.2; do
      origin=$(manifest_entry_field "$manifest" "$path" source_origin)
      source_path=$(manifest_entry_field "$manifest" "$path" source_path)
      if [ "$origin" = "build_testroot" ]; then
        printf 'network-facing public payload %s still uses build_testroot origin in %s\n' "$path" "$manifest" >&2
        exit 1
      fi
      case "$source_path" in
        build/testroot.pristine/usr/lib64/*)
          printf 'network-facing public payload %s still points at baseline public source %s in %s\n' "$path" "$source_path" "$manifest" >&2
          exit 1
          ;;
      esac
    done
  done

  log "Checking explicit network backend inventory"
  for path in \
    /usr/libexec/safelibs/backends/libanl.so.1 \
    /usr/libexec/safelibs/backends/libnsl.so.1 \
    /usr/libexec/safelibs/backends/libnss_compat.so.2 \
    /usr/libexec/safelibs/backends/libnss_dns.so.2 \
    /usr/libexec/safelibs/backends/libnss_files.so.2 \
    /usr/libexec/safelibs/backends/libnss_hesiod.so.2 \
    /usr/libexec/safelibs/backends/libresolv.so.2; do
    test -e "$path"
  done

  log "Checking network tool entrypoints"
  test -x /usr/bin/getent
  test -x /usr/sbin/nscd
  if [ -e /usr/libexec/safelibs/fallback/libc-bin/getent.real ]; then
    printf 'temporary getent fallback payload should no longer ship\n' >&2
    exit 1
  fi
  if [ -e /usr/libexec/safelibs/fallback/nscd/nscd.real ]; then
    printf 'temporary nscd fallback payload should no longer ship\n' >&2
    exit 1
  fi

  /usr/bin/getent passwd root >/tmp/getent-passwd.txt
  /usr/bin/getent group root >/tmp/getent-group.txt
  /usr/bin/getent hosts localhost >/tmp/getent-hosts.txt
  /usr/bin/getent ahostsv4 localhost >/tmp/getent-ahostsv4.txt
  if [ ! -s /tmp/getent-passwd.txt ] || [ ! -s /tmp/getent-group.txt ] || [ ! -s /tmp/getent-hosts.txt ]; then
    printf 'getent smoke output was unexpectedly empty\n' >&2
    exit 1
  fi

  /usr/sbin/nscd --version >/tmp/nscd-version.txt
  /usr/sbin/nscd -g >/tmp/nscd-status-before.txt || true
  /usr/sbin/nscd --foreground >/tmp/nscd-foreground.txt 2>&1 &
  nscd_pid=$!
  sleep 2
  test -f /run/nscd/nscd.pid
  /usr/sbin/nscd -i hosts
  /usr/sbin/nscd -i passwd
  /usr/sbin/nscd --shutdown
  wait "$nscd_pid"
  if [ ! -s /tmp/nscd-version.txt ]; then
    printf 'nscd version output was empty\n' >&2
    exit 1
  fi
}

smoke_locale_tools() {
  log "Verifying locale-tool payloads from the committed install manifest"
  jq -r '.entries[] | select(.verification == "locale-tools" and .shipped_status == "shipped") | [.package, .path] | @tsv' \
    /workspace/safe/generated/install-manifests/required-packages.json | \
  while IFS=$'\t' read -r pkg path; do
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
      printf 'missing installed locale-tool payload for %s: %s\n' "$pkg" "$path" >&2
      exit 1
    fi
  done

  log "Checking locale-tool entrypoints and backend inventory"
  for path in \
    /usr/bin/iconv \
    /usr/bin/locale \
    /usr/bin/localedef \
    /usr/sbin/iconvconfig \
    /usr/sbin/locale-gen \
    /usr/sbin/update-locale \
    /usr/sbin/validlocale \
    /usr/share/locales/install-language-pack \
    /usr/share/locales/remove-language-pack; do
    test -x "$path"
  done
  for path in \
    /usr/libexec/safelibs/locale-tools/iconv.backend \
    /usr/libexec/safelibs/locale-tools/iconvconfig.backend \
    /usr/libexec/safelibs/locale-tools/locale.backend \
    /usr/libexec/safelibs/locale-tools/localedef.backend; do
    test -x "$path"
  done
  for path in \
    /usr/libexec/safelibs/fallback/libc-bin/iconv.real \
    /usr/libexec/safelibs/fallback/libc-bin/locale.real \
    /usr/libexec/safelibs/fallback/libc-bin/localedef.real \
    /usr/libexec/safelibs/fallback/libc-bin/iconvconfig.real \
    /usr/libexec/safelibs/fallback/locales/locale-gen.real \
    /usr/libexec/safelibs/fallback/locales/update-locale.real \
    /usr/libexec/safelibs/fallback/locales/validlocale.real \
    /usr/libexec/safelibs/fallback/locales/install-language-pack.real \
    /usr/libexec/safelibs/fallback/locales/remove-language-pack.real; do
    if [ -e "$path" ]; then
      printf 'phase-08 locale helper still ships fallback payload %s\n' "$path" >&2
      exit 1
    fi
  done

  log "Checking locale helper behavior from the installed packages"
  printf 'hello world\n' | /usr/bin/iconv -f UTF-8 -t UTF-8 >/tmp/iconv-roundtrip.txt
  grep -qx 'hello world' /tmp/iconv-roundtrip.txt

  /usr/bin/locale charmap >/tmp/locale-charmap.txt
  /usr/bin/locale -a >/tmp/locale-list.txt
  /usr/bin/locale >/tmp/locale-defaults.txt
  test -s /tmp/locale-charmap.txt
  test -s /tmp/locale-list.txt
  test -s /tmp/locale-defaults.txt

  /usr/sbin/iconvconfig --help >/tmp/iconvconfig-help.txt
  /usr/bin/localedef --help >/tmp/localedef-help.txt
  test -s /tmp/iconvconfig-help.txt
  test -s /tmp/localedef-help.txt

  mkdir -p /var/lib/locales/supported.d
  if ! grep -Eq '^[# ]*en_US.UTF-8 UTF-8$' /etc/locale.gen; then
    printf '\nen_US.UTF-8 UTF-8\n' >> /etc/locale.gen
  fi
  /usr/sbin/locale-gen --keep-existing en_US.UTF-8 >/tmp/locale-gen.txt
  /usr/bin/locale -a | tr 'A-Z' 'a-z' | grep -qx 'en_us.utf8'
  /usr/sbin/update-locale LANG=en_US.UTF-8 LANGUAGE=en_US:en >/tmp/update-locale.txt 2>&1 || true
  grep -Eq '^LANG=en_US.UTF-8$' /etc/locale.conf
  test -L /etc/default/locale
  test "$(readlink -f /etc/default/locale)" = "/etc/locale.conf"
  /usr/sbin/validlocale en_US.UTF-8 >/tmp/validlocale.txt 2>&1
  /usr/share/locales/install-language-pack en >/tmp/install-language-pack.txt 2>&1
  /usr/share/locales/remove-language-pack en >/tmp/remove-language-pack.txt 2>&1 || true
}

smoke_libc_family_cutover() {
  log "Checking libc-family manifest provenance cutover"
  for manifest in \
    /workspace/safe/generated/baseline/package-files/libc6.json \
    /workspace/safe/generated/baseline/package-files/libc6-dev.json \
    /workspace/safe/generated/install-manifests/required-packages.json; do
    for path in \
      /usr/lib64/ld-linux-x86-64.so.2 \
      /usr/lib64/libBrokenLocale.so.1 \
      /usr/lib64/libc.so.6 \
      /usr/lib64/libpthread.so.0 \
      /usr/lib64/libthread_db.so.1 \
      /usr/lib64/libc_malloc_debug.so.0 \
      /usr/lib64/libmemusage.so \
      /usr/lib64/libBrokenLocale.so \
      /usr/lib64/libc.so \
      /usr/lib64/libthread_db.so \
      /usr/lib64/libc_malloc_debug.so; do
      if jq -e --arg path "$path" '.entries[] | select(.path == $path)' "$manifest" >/dev/null; then
        origin=$(manifest_entry_field "$manifest" "$path" source_origin)
        source_path=$(manifest_entry_field "$manifest" "$path" source_path)
        if [ "$origin" = "build_testroot" ]; then
          printf 'public phase-06 payload %s still uses build_testroot origin in %s\n' "$path" "$manifest" >&2
          exit 1
        fi
        case "$source_path" in
          build/testroot.pristine/usr/lib64/*)
            printf 'public phase-06 payload %s still points at baseline public source %s in %s\n' "$path" "$source_path" "$manifest" >&2
            exit 1
            ;;
        esac
      fi
    done
  done

  log "Checking explicit private backend inventory"
  for path in \
    /usr/libexec/safelibs/backends/ld-linux-x86-64.so.2 \
    /usr/libexec/safelibs/backends/libc.so.6 \
    /usr/libexec/safelibs/backends/libpthread.so.0 \
    /usr/libexec/safelibs/backends/libthread_db.so.1 \
    /usr/libexec/safelibs/backends/libc_malloc_debug.so.0 \
    /usr/libexec/safelibs/backends/libmemusage.so; do
    test -f "$path"
  done

  log "Comparing installed public payloads with staged safe-build sources"
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/ld-linux-x86-64.so.2
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/libBrokenLocale.so.1
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/libc.so.6
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/libpthread.so.0
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/libthread_db.so.1
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/libc_malloc_debug.so.0
  compare_public_payload /workspace/safe/generated/install-manifests/required-packages.json /usr/lib64/libmemusage.so
  compare_public_payload /workspace/safe/generated/baseline/package-files/libc6-dev.json /usr/lib64/libBrokenLocale.so
  compare_public_payload /workspace/safe/generated/baseline/package-files/libc6-dev.json /usr/lib64/libc.so
  compare_public_payload /workspace/safe/generated/baseline/package-files/libc6-dev.json /usr/lib64/libthread_db.so
  compare_public_payload /workspace/safe/generated/baseline/package-files/libc6-dev.json /usr/lib64/libc_malloc_debug.so

  smoke_loader_tools
  smoke_runtime_tools
}

log "Updating base package indexes"
apt-get update

log "Installing bootstrap tools"
apt-get install -y --no-install-recommends ca-certificates jq dpkg-dev binutils file debconf

log "Configuring the local safe apt repository"
/workspace/safe/scripts/install-safe-repo.sh "$deb_dir_rel"

log "Installing the required safe package set"
apt-get install -y --no-install-recommends --allow-downgrades \
  libc6 libc6-dev libc6-dbg libc-bin libc-dev-bin locales nscd

verify_safe_provenance
case "${SAFE_SMOKE_SET}" in
  basic-required-packages)
    smoke_basic_required_packages
    ;;
  libc-family-cutover)
    smoke_libc_family_cutover
    ;;
  loader-tools)
    smoke_loader_tools
    ;;
  runtime-tools)
    smoke_runtime_tools
    ;;
  network-tools)
    smoke_network_tools
    ;;
  locale-tools)
    smoke_locale_tools
    ;;
  *)
    printf 'unsupported smoke set in container: %s\n' "${SAFE_SMOKE_SET}" >&2
    exit 1
    ;;
esac
"#;

    let output = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg("-i")
        .arg("--privileged")
        .arg("-e")
        .arg("DEBIAN_FRONTEND=noninteractive")
        .arg("-e")
        .arg(format!("SAFE_DEB_DIR_REL={deb_dir_rel}"))
        .arg("-e")
        .arg(format!("SAFE_VERSION={}", safe_package_version()?))
        .arg("-e")
        .arg(format!("SAFE_SMOKE_SET={}", args.smoke_set))
        .arg("-v")
        .arg(format!("{}:/workspace:ro", repo_root().display()))
        .arg("-w")
        .arg("/workspace")
        .arg(&args.docker_image)
        .arg("bash")
        .arg("-lc")
        .arg(script)
        .output()
        .with_context(|| format!("failed to start docker image {}", args.docker_image))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "test-package-install failed ({}):\n{}\n{}",
            output.status,
            stdout,
            stderr
        );
    }
    Ok(())
}

fn resolve_safe_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        safe_root().join(path)
    }
}
