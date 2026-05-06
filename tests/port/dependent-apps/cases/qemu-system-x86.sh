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

dependent_apps_require_command qemu-system-x86_64
pid_file="$scratch_root/qemu.pid"

cleanup_qemu() {
  if [[ -s "$pid_file" ]]; then
    pid=$(cat "$pid_file")
    kill "$pid" >/dev/null 2>&1 || true
  fi
}
trap cleanup_qemu EXIT

qemu-system-x86_64 -machine none -nodefaults -display none -nographic \
  -monitor none -serial none -accel tcg -daemonize -pidfile "$pid_file"
timeout 10 bash -c 'while [[ ! -s "$1" ]]; do sleep 0.1; done' bash "$pid_file"
pid=$(cat "$pid_file")
kill -0 "$pid"
kill "$pid"
if ! timeout 10 bash -c 'while kill -0 "$1" 2>/dev/null; do sleep 0.1; done' bash "$pid"; then
  kill -9 "$pid" >/dev/null 2>&1 || true
  exit 1
fi
trap - EXIT
