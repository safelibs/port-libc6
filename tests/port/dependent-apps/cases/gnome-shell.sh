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

dependent_apps_require_command dbus-daemon
dependent_apps_require_command dbus-run-session
dependent_apps_require_command Xvfb
dependent_apps_require_command gnome-shell

runtime_dir="$scratch_root/gnome-runtime"
gnome_log="$scratch_root/gnome-shell.log"
logind_log="$scratch_root/gnome-logind.log"
xvfb_log="$scratch_root/gnome-xvfb.log"
dbus_pid=""
logind_pid=""
xvfb_pid=""

cleanup_gnome_shell() {
  if [[ -n "$xvfb_pid" ]]; then
    kill "$xvfb_pid" >/dev/null 2>&1 || true
    wait "$xvfb_pid" 2>/dev/null || true
  fi
  if [[ -n "$logind_pid" ]]; then
    kill "$logind_pid" >/dev/null 2>&1 || true
    wait "$logind_pid" 2>/dev/null || true
  fi
  if [[ -n "$dbus_pid" ]]; then
    kill "$dbus_pid" >/dev/null 2>&1 || true
    wait "$dbus_pid" 2>/dev/null || true
  fi
  rm -f /run/dbus/pid /tmp/.X99-lock
}
trap cleanup_gnome_shell EXIT

rm -f /run/dbus/pid /tmp/.X99-lock
mkdir -p /run/dbus "$runtime_dir"
chmod 700 "$runtime_dir"
dbus-daemon --system --fork >/dev/null 2>"$scratch_root/gnome-system-dbus.log"
dbus_pid=$(cat /run/dbus/pid)
/lib/systemd/systemd-logind >"$logind_log" 2>&1 &
logind_pid=$!
Xvfb :99 -screen 0 1024x768x24 >"$xvfb_log" 2>&1 &
xvfb_pid=$!

DISPLAY=:99 XDG_RUNTIME_DIR="$runtime_dir" timeout 30 dbus-run-session -- \
  bash -lc '
    gnome-shell --x11 --replace >"$1" 2>&1 &
    shell_pid=$!
    for _ in $(seq 1 20); do
      if grep -Fq "GNOME Shell started" "$1"; then
        kill "$shell_pid" >/dev/null 2>&1 || true
        wait "$shell_pid" 2>/dev/null || true
        exit 0
      fi
      kill -0 "$shell_pid" 2>/dev/null || exit 1
      sleep 1
    done
    kill "$shell_pid" >/dev/null 2>&1 || true
    wait "$shell_pid" 2>/dev/null || true
    exit 1
  ' bash "$gnome_log"

grep -F 'GNOME Shell started' "$gnome_log"
trap - EXIT
cleanup_gnome_shell
