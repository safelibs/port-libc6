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

dependent_apps_require_command nmcli
nm_output="$scratch_root/nmcli-offline.txt"
nmcli --offline connection add type ethernet con-name test-eth ifname eth0 \
  ipv4.method auto >"$nm_output"
grep -F 'id=test-eth' "$nm_output"
