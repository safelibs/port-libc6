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

dependent_apps_require_command systemd-analyze
dependent_apps_require_command systemd-tmpfiles

unit_file="$scratch_root/systemd-demo.service"
tmpfiles_file="$scratch_root/systemd-demo.tmpfiles"
tmpfiles_dir="$scratch_root/systemd-tmpfiles-check"

cat >"$unit_file" <<'SYSTEMD_UNIT'
[Unit]
Description=SafeLibs dependent app smoke service

[Service]
Type=oneshot
ExecStart=/usr/bin/true

[Install]
WantedBy=multi-user.target
SYSTEMD_UNIT

cat >"$tmpfiles_file" <<SYSTEMD_TMPFILES
d $tmpfiles_dir 0755 root root - -
SYSTEMD_TMPFILES

dependent_apps_log "verifying unit file"
timeout 20 systemd-analyze verify "$unit_file"
dependent_apps_log "creating tmpfiles entry"
timeout 20 systemd-tmpfiles --create "$tmpfiles_file"
[[ -d "$tmpfiles_dir" ]]
