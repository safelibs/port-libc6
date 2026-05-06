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

dependent_apps_require_command podman
storage_root="/tmp/podman-root"
run_root="/tmp/podman-run"
image_ref="docker.io/library/alpine:3.19"
rm -rf "$storage_root" "$run_root"
podman_base=(
  podman
  --root "$storage_root"
  --runroot "$run_root"
  --storage-driver=vfs
  --events-backend=file
  --cgroup-manager=cgroupfs
)

"${podman_base[@]}" pull --tls-verify=false "$image_ref"
output=$("${podman_base[@]}" run --network=none --cgroups=disabled --rm "$image_ref" \
  /bin/sh -c 'printf podman-ok')
printf 'podman_output=%s\n' "$output"
[[ "$output" == "podman-ok" ]]
