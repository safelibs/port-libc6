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

dependent_apps_require_command ssh
dependent_apps_require_command ssh-keygen
ssh_dir="$scratch_root/ssh"
config_dump="$ssh_dir/ssh-config.txt"
mkdir -p "$ssh_dir"
ssh-keygen -q -t ed25519 -N '' -f "$ssh_dir/testkey"
ssh -G localhost >"$config_dump" 2>/dev/null
grep -F 'hostname localhost' "$config_dump"
