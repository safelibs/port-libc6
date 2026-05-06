#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${ROOT_DIR:-}" ]]; then
  ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
fi
HARNESS_DIR="${HARNESS_DIR:-$ROOT_DIR/tests/port/dependent-apps}"
# shellcheck source=tests/port/dependent-apps/lib/common.sh
. "$HARNESS_DIR/lib/common.sh"

case_name="${DEPENDENT_APPS_CASE:-$(basename "${BASH_SOURCE[0]}" .sh)}"
dependent_apps_case_init "$case_name"
scratch_root="$DEPENDENT_APPS_CASE_WORKDIR"

dependent_apps_require_command libvirtd
dependent_apps_require_command virt-admin
dependent_apps_require_command groupadd
dependent_apps_require_command useradd

libvirt_root="$scratch_root/libvirt-daemon-check"
run_dir="$libvirt_root/run"
config_file="$libvirt_root/libvirtd.conf"
pid_file="$libvirt_root/libvirtd.pid"
daemon_log="$libvirt_root/libvirtd.log"
admin_log="$libvirt_root/virt-admin.log"
daemon_pid=""

cleanup_libvirt() {
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" 2>/dev/null || true
  fi
}
trap cleanup_libvirt EXIT

getent group libvirt-qemu >/dev/null || groupadd --system libvirt-qemu
id -u libvirt-qemu >/dev/null 2>&1 || \
  useradd --system --gid libvirt-qemu --home-dir /var/lib/libvirt/qemu --create-home libvirt-qemu
getent group kvm >/dev/null || groupadd --system kvm

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

LIBVIRT_LOG_OUTPUTS="1:file:$daemon_log" \
  libvirtd -d -t 30 -f "$config_file" -p "$pid_file"

timeout 10 bash -c 'while [[ ! -s "$1" ]]; do sleep 0.1; done' bash "$pid_file"
daemon_pid=$(cat "$pid_file")
virt-admin -c "virtadm+unix:///system?socket=$run_dir/libvirt-admin-sock" \
  srv-list >"$admin_log" 2>&1

grep -Eq '^[[:space:]]*[0-9]+[[:space:]]+admin$' "$admin_log"
grep -Eq '^[[:space:]]*[0-9]+[[:space:]]+libvirtd$' "$admin_log"

cleanup_libvirt
daemon_pid=""
trap - EXIT
