#!/usr/bin/env bash

SOURCE_BUILD_MANIFEST_PATH="${ROOT_DIR:?ROOT_DIR must be set before sourcing source-build-common.sh}/dependents.json"
SOURCE_BUILD_FAILURE_KIND_PATH=""
SOURCE_BUILD_SOURCE_PACKAGE=""
SOURCE_BUILD_SOURCE_VERSION=""
SOURCE_BUILD_BINARY_PACKAGE=""
SOURCE_BUILD_PACKAGE_ROOT=""
SOURCE_BUILD_SRC_DIR=""
SOURCE_BUILD_INSTALL_ROOT=""

source_build_log() {
  printf '\n==> %s\n' "$*"
}

source_build_print_command() {
  printf 'command:'
  printf ' %q' "$@"
  printf '\n'
}

source_build_mark_failure() {
  local failure_kind=$1
  if [[ -n "$SOURCE_BUILD_FAILURE_KIND_PATH" ]]; then
    mkdir -p "$(dirname "$SOURCE_BUILD_FAILURE_KIND_PATH")"
    printf '%s\n' "$failure_kind" >"$SOURCE_BUILD_FAILURE_KIND_PATH"
  fi
}

source_build_fail() {
  local failure_kind=$1
  shift
  source_build_mark_failure "$failure_kind"
  printf 'source-build %s failure: %s\n' "$failure_kind" "$*" >&2
  exit 1
}

source_build_run_harness() {
  local description=$1
  local rc
  shift
  source_build_log "$description"
  source_build_print_command "$@"
  set +e
  "$@"
  rc=$?
  set -e
  if (( rc == 0 )); then
    return 0
  fi
  source_build_mark_failure harness
  printf 'harness command failed with exit code %s: %s\n' "$rc" "$description" >&2
  exit "$rc"
}

source_build_run_compat() {
  local description=$1
  local rc
  shift
  source_build_log "$description"
  source_build_print_command "$@"
  set +e
  "$@"
  rc=$?
  set -e
  if (( rc == 0 )); then
    return 0
  fi
  source_build_mark_failure compatibility_candidate
  printf 'compatibility candidate command failed with exit code %s: %s\n' "$rc" "$description" >&2
  exit "$rc"
}

source_build_load_metadata() {
  local case_name=$1

  SOURCE_BUILD_SOURCE_PACKAGE=$(jq -r --arg name "$case_name" '
    .dependents[] | select(.name == $name) | .source_package // empty
  ' "$SOURCE_BUILD_MANIFEST_PATH")
  SOURCE_BUILD_SOURCE_VERSION=$(jq -r --arg name "$case_name" '
    .dependents[] | select(.name == $name) | .source_version // empty
  ' "$SOURCE_BUILD_MANIFEST_PATH")
  SOURCE_BUILD_BINARY_PACKAGE=$(jq -r --arg name "$case_name" '
    .dependents[] | select(.name == $name) | .binary_package // empty
  ' "$SOURCE_BUILD_MANIFEST_PATH")

  [[ -n "$SOURCE_BUILD_SOURCE_PACKAGE" ]] || \
    source_build_fail harness "missing source_package for $case_name"
  [[ -n "$SOURCE_BUILD_SOURCE_VERSION" ]] || \
    source_build_fail harness "missing source_version for $case_name"
  [[ -n "$SOURCE_BUILD_BINARY_PACKAGE" ]] || \
    source_build_fail harness "missing binary_package for $case_name"

  if ! jq -e --arg name "$case_name" '
    .dependents[] |
    select(.name == $name) |
    any(.dependency_modes[]?; . == "compile_time_via_glibc_dev")
  ' "$SOURCE_BUILD_MANIFEST_PATH" >/dev/null; then
    source_build_fail harness "$case_name is not declared compile_time_via_glibc_dev"
  fi
}

source_build_case_init() {
  local case_name=$1

  dependent_apps_case_init "$case_name"
  SOURCE_BUILD_FAILURE_KIND_PATH="${DEPENDENT_APPS_FAILURE_KIND_PATH:-$DEPENDENT_APPS_CASE_WORKDIR/failure-kind}"
  mkdir -p "$(dirname "$SOURCE_BUILD_FAILURE_KIND_PATH")"
  rm -f "$SOURCE_BUILD_FAILURE_KIND_PATH"

  source_build_load_metadata "$case_name"

  SOURCE_BUILD_PACKAGE_ROOT="$DEPENDENT_APPS_CASE_WORKDIR/source/$SOURCE_BUILD_SOURCE_PACKAGE"
  SOURCE_BUILD_INSTALL_ROOT="$SOURCE_BUILD_PACKAGE_ROOT/install"
  SOURCE_BUILD_SRC_DIR=""
}

source_build_enable_source_repositories() {
  cat >/etc/apt/sources.list.d/ubuntu-src.sources <<'SRC'
Types: deb-src
URIs: http://archive.ubuntu.com/ubuntu/
Suites: noble noble-updates noble-backports
Components: main universe restricted multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb-src
URIs: http://security.ubuntu.com/ubuntu/
Suites: noble-security
Components: main universe restricted multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SRC
}

source_build_ensure_safe_repo() {
  if [[ -d /tmp/safelibs-apt-repo && -f /etc/apt/sources.list.d/safelibs-local.list ]]; then
    return 0
  fi
  ROOT_DIR=/workspace /workspace/safe/scripts/install-safe-repo.sh safe/work/debs
}

source_build_log_safe_package_state() {
  local label=$1
  local policy
  local libc6_version
  local libc_dev_version
  local libc_bin_version

  source_build_log "safe package state: $label"
  printf 'dpkg-query -W libc6 libc6-dev libc-bin\n'
  dpkg-query -W -f='${Package}\t${Version}\n' libc6 libc6-dev libc-bin

  printf '\napt-cache policy libc6 libc6-dev\n'
  policy=$(apt-cache policy libc6 libc6-dev)
  printf '%s\n' "$policy"

  libc6_version=$(dpkg-query -W -f='${Version}' libc6)
  libc_dev_version=$(dpkg-query -W -f='${Version}' libc6-dev)
  libc_bin_version=$(dpkg-query -W -f='${Version}' libc-bin)
  printf 'selected libc6=%s\n' "$libc6_version"
  printf 'selected libc6-dev=%s\n' "$libc_dev_version"
  printf 'selected libc-bin=%s\n' "$libc_bin_version"

  [[ "$libc6_version" == "$SAFE_VERSION" ]] || \
    source_build_fail harness "selected libc6 version $libc6_version does not match $SAFE_VERSION"
  [[ "$libc_dev_version" == "$SAFE_VERSION" ]] || \
    source_build_fail harness "selected libc6-dev version $libc_dev_version does not match $SAFE_VERSION"
  [[ "$libc_bin_version" == "$SAFE_VERSION" ]] || \
    source_build_fail harness "selected libc-bin version $libc_bin_version does not match $SAFE_VERSION"
  grep -Fq 'file:/tmp/safelibs-apt-repo' <<<"$policy" || \
    source_build_fail harness 'apt policy does not show file:/tmp/safelibs-apt-repo'
}

source_build_install_builddep_compat_shims() {
  local upstream_version=${SAFE_VERSION%%+safelibs*}
  local shim_root="$DEPENDENT_APPS_CASE_WORKDIR/builddep-shims"
  local deb_dir="$shim_root/debs"
  local pkg_root
  local debian_dir

  rm -rf "$shim_root"
  mkdir -p "$deb_dir"

  create_shim() {
    local package=$1
    local depends=$2
    pkg_root="$shim_root/$package"
    debian_dir="$pkg_root/DEBIAN"
    mkdir -p "$debian_dir"
    cat >"$debian_dir/control" <<SHIM
Package: $package
Version: $upstream_version
Architecture: amd64
Maintainer: SafeLibs <noreply@safelibs.local>
Section: devel
Priority: optional
Depends: $depends
Description: SafeLibs dependent-harness build-dependency shim for $package
 This package is created inside the dependent-app source-build harness. It
 satisfies Ubuntu source build-dependencies that require exact-version multilib
 libc companion packages without expanding the final SafeLibs package payload.
SHIM
    dpkg-deb --build "$pkg_root" \
      "$deb_dir/${package}_${upstream_version}_amd64.deb" >/dev/null
  }

  create_shim libc6-i386 "libc6 (>= $upstream_version)"
  create_shim libc6-x32 "libc6 (>= $upstream_version)"
  create_shim libc6-dev-i386 "libc6-dev (>= $upstream_version), libc6-i386 (= $upstream_version)"
  create_shim libc6-dev-x32 "libc6-dev (>= $upstream_version), libc6-x32 (= $upstream_version)"

  dpkg -i "$deb_dir"/*.deb
}

source_build_prepare_dependencies() {
  source_build_run_harness "enable Ubuntu source repositories" \
    source_build_enable_source_repositories
  source_build_run_harness "ensure local SafeLibs apt repository" \
    source_build_ensure_safe_repo
  source_build_run_harness "apt-get update command" \
    apt-get update
  source_build_run_harness "install source-build bootstrap tools" \
    apt-get install -y --no-install-recommends ca-certificates jq build-essential dpkg-dev fakeroot
  source_build_log_safe_package_state "before build dependencies"
  source_build_run_harness "install build-dependency compatibility shims" \
    source_build_install_builddep_compat_shims
  source_build_run_harness "install build dependencies command: apt-get build-dep -y $SOURCE_BUILD_SOURCE_PACKAGE" \
    apt-get build-dep -y "$SOURCE_BUILD_SOURCE_PACKAGE"
  source_build_log_safe_package_state "after build dependencies"
}

source_build_fetch_source() {
  local dsc_file
  local fetched_version

  rm -rf "$SOURCE_BUILD_PACKAGE_ROOT"
  mkdir -p "$SOURCE_BUILD_PACKAGE_ROOT"

  source_build_log "fetch source command: apt-get source $SOURCE_BUILD_SOURCE_PACKAGE=$SOURCE_BUILD_SOURCE_VERSION"
  source_build_print_command apt-get source "${SOURCE_BUILD_SOURCE_PACKAGE}=${SOURCE_BUILD_SOURCE_VERSION}"
  set +e
  (cd "$SOURCE_BUILD_PACKAGE_ROOT" && apt-get source "${SOURCE_BUILD_SOURCE_PACKAGE}=${SOURCE_BUILD_SOURCE_VERSION}")
  local rc=$?
  set -e
  if (( rc != 0 )); then
    printf 'exact source fetch failed with exit code %s; retrying unversioned fetch for diagnostics\n' "$rc" >&2
    (cd "$SOURCE_BUILD_PACKAGE_ROOT" && apt-get source "$SOURCE_BUILD_SOURCE_PACKAGE") || true
  fi

  dsc_file=$(find "$SOURCE_BUILD_PACKAGE_ROOT" -maxdepth 1 -type f -name '*.dsc' | sort | head -n 1)
  SOURCE_BUILD_SRC_DIR=$(find "$SOURCE_BUILD_PACKAGE_ROOT" -mindepth 1 -maxdepth 1 -type d | sort | head -n 1)
  [[ -n "$dsc_file" && -n "$SOURCE_BUILD_SRC_DIR" ]] || \
    source_build_fail harness "unable to locate fetched source package for $SOURCE_BUILD_SOURCE_PACKAGE"
  mkdir -p "$SOURCE_BUILD_INSTALL_ROOT"

  if ! fetched_version=$(dpkg-parsechangelog -l "$SOURCE_BUILD_SRC_DIR/debian/changelog" -S Version); then
    source_build_fail harness "unable to read fetched source version for $SOURCE_BUILD_SOURCE_PACKAGE"
  fi
  printf 'exact source package version fetched: %s\n' "$fetched_version"
  printf 'declared source package version: %s\n' "$SOURCE_BUILD_SOURCE_VERSION"
  printf 'source dsc: %s\n' "$dsc_file"
  printf 'source directory: %s\n' "$SOURCE_BUILD_SRC_DIR"
  if [[ "$fetched_version" != "$SOURCE_BUILD_SOURCE_VERSION" ]]; then
    printf 'fetched source package version differs from dependents.json: expected %s, got %s\n' \
      "$SOURCE_BUILD_SOURCE_VERSION" "$fetched_version"
  fi
}

source_build_prepare_case() {
  source_build_prepare_dependencies
  source_build_fetch_source
}

source_build_smoke_strace() {
  local strace_bin=$1
  local stdout_file="$DEPENDENT_APPS_CASE_WORKDIR/strace.stdout"
  local trace_file="$DEPENDENT_APPS_CASE_WORKDIR/strace.out"

  source_build_log "final smoke command: $strace_bin -o $trace_file -e write bash -lc 'printf traced'"
  "$strace_bin" -o "$trace_file" -e write bash -lc 'printf traced' >"$stdout_file"
  printf 'smoke stdout:\n'
  cat "$stdout_file"
  printf '\ntrace excerpt:\n'
  grep -F 'write(1, "traced"' "$trace_file"
}

source_build_smoke_valgrind() {
  local valgrind_bin=$1
  local valgrind_lib=$2
  local src_file="$DEPENDENT_APPS_CASE_WORKDIR/valgrind-smoke.c"
  local exe_file="$DEPENDENT_APPS_CASE_WORKDIR/valgrind-smoke"
  local smoke_log="$DEPENDENT_APPS_CASE_WORKDIR/valgrind.log"

  cat >"$src_file" <<'SRC'
#include <stdlib.h>

int main(void) {
  int *p = malloc(sizeof(*p));
  if (p == NULL) {
    return 1;
  }
  *p = 42;
  free(p);
  return 0;
}
SRC

  source_build_log "final smoke command: VALGRIND_LIB=$valgrind_lib $valgrind_bin --error-exitcode=1 --leak-check=full $exe_file"
  cc -O0 -g "$src_file" -o "$exe_file"
  VALGRIND_LIB="$valgrind_lib" \
    "$valgrind_bin" --error-exitcode=1 --leak-check=full "$exe_file" \
    >"$smoke_log" 2>&1
  printf 'smoke output:\n'
  cat "$smoke_log"
  grep -F 'ERROR SUMMARY: 0 errors' "$smoke_log"
}

source_build_smoke_libvirt() {
  local libvirtd_bin=$1
  local virt_admin_bin=$2
  local lib_path=$3
  local libvirt_root="$DEPENDENT_APPS_CASE_WORKDIR/libvirt-daemon-check"
  local run_dir="$libvirt_root/run"
  local config_file="$libvirt_root/libvirtd.conf"
  local pid_file="$libvirt_root/libvirtd.pid"
  local daemon_log="$libvirt_root/libvirtd.log"
  local admin_log="$libvirt_root/virt-admin.log"
  local daemon_pid=""

  cleanup_libvirt() {
    if [[ -n "$daemon_pid" ]]; then
      kill "$daemon_pid" >/dev/null 2>&1 || true
      wait "$daemon_pid" 2>/dev/null || true
    fi
  }
  trap cleanup_libvirt RETURN

  getent group libvirt-qemu >/dev/null || groupadd --system libvirt-qemu
  id -u libvirt-qemu >/dev/null 2>&1 || \
    useradd --system --gid libvirt-qemu --home-dir /var/lib/libvirt/qemu --create-home libvirt-qemu
  getent group kvm >/dev/null || groupadd --system kvm

  rm -rf "$libvirt_root"
  mkdir -p "$run_dir" "$libvirt_root/cache" "$libvirt_root/log"
  cat >"$config_file" <<LIBVIRT_CONF
listen_tls = 0
listen_tcp = 0
unix_sock_dir = "$run_dir"
unix_sock_group = "root"
unix_sock_ro_perms = "0777"
unix_sock_rw_perms = "0770"
auth_unix_ro = "none"
auth_unix_rw = "none"
LIBVIRT_CONF

  source_build_log "libvirt daemon smoke setup command: LD_LIBRARY_PATH=$lib_path LIBVIRT_LOG_OUTPUTS=1:file:$daemon_log $libvirtd_bin -d -t 30 -f $config_file -p $pid_file"
  LD_LIBRARY_PATH="$lib_path${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    LIBVIRT_LOG_OUTPUTS="1:file:$daemon_log" \
    "$libvirtd_bin" -d -t 30 -f "$config_file" -p "$pid_file"

  timeout 10 bash -c 'while [[ ! -s "$1" ]]; do sleep 0.1; done' bash "$pid_file"
  daemon_pid=$(cat "$pid_file")

  source_build_log "final smoke command: LD_LIBRARY_PATH=$lib_path $virt_admin_bin -c virtadm+unix:///system?socket=$run_dir/libvirt-admin-sock srv-list"
  LD_LIBRARY_PATH="$lib_path${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    "$virt_admin_bin" \
    -c "virtadm+unix:///system?socket=$run_dir/libvirt-admin-sock" \
    srv-list >"$admin_log" 2>&1
  printf 'smoke output:\n'
  cat "$admin_log"

  grep -Eq '^[[:space:]]*[0-9]+[[:space:]]+admin$' "$admin_log"
  grep -Eq '^[[:space:]]*[0-9]+[[:space:]]+libvirtd$' "$admin_log"

  cleanup_libvirt
  daemon_pid=""
  trap - RETURN
}
