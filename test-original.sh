#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
HARNESS_DIR="$ROOT_DIR/tests/port/dependent-apps"
SAFE_DEB_DIR_REL="${SAFE_DEB_DIR_REL:-safe/work/debs}"

usage() {
  cat <<'USAGE' >&2
Usage: test-original.sh [dependent-app run.sh arguments]

With no arguments, builds the SafeLibs libc6 package set, builds the dependent
application image, then runs image-contract, all-runtime, and source-builds.
With arguments, forwards directly to tests/port/dependent-apps/run.sh.
USAGE
}

require_command() {
  local command_name=$1
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'test-original.sh: %s is required\n' "$command_name" >&2
    exit 1
  }
}

run_suite() {
  local suite=$1
  shift
  if ! "$HARNESS_DIR/run.sh" --image "$IMAGE" --suite "$suite" "$@"; then
    runner_status=1
  fi
}

if (($# > 0)); then
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    *)
      exec "$HARNESS_DIR/run.sh" "$@"
      ;;
  esac
fi

require_command docker
require_command jq
require_command sed

printf 'Building safe package set into %s\n' "$SAFE_DEB_DIR_REL"
"$ROOT_DIR/safe/scripts/build-debs.sh"

SAFE_VERSION=$(jq -r '.safe_package_version' \
  "$ROOT_DIR/safe/generated/packaging/package-build-manifest.json")
SAFE_IMAGE_TAG=$(printf '%s' "$SAFE_VERSION" | sed 's/[^A-Za-z0-9_.-]/-/g')
IMAGE="${DEPENDENT_APPS_IMAGE:-safelibs-libc6-dependent:$SAFE_IMAGE_TAG}"

printf 'Building dependent application image %s\n' "$IMAGE"
"$HARNESS_DIR/build-image.sh" \
  --debs "$SAFE_DEB_DIR_REL" \
  --manifest "$ROOT_DIR/dependents.json" \
  --tag "$IMAGE"

runner_status=0
run_suite image-contract
run_suite all-runtime --privileged
run_suite source-builds --privileged

result_files=(
  "$ROOT_DIR/safe/work/dependent-apps/results/image-contract.json"
  "$ROOT_DIR/safe/work/dependent-apps/results/all-runtime.json"
  "$ROOT_DIR/safe/work/dependent-apps/results/source-builds.json"
)

for result_file in "${result_files[@]}"; do
  if [[ ! -f "$result_file" ]]; then
    printf 'missing dependent-app result: %s\n' "$result_file" >&2
    runner_status=1
  fi
done

if (( runner_status != 0 )); then
  exit 1
fi

failed_total=$(jq -s '[.[].summary.failed] | add' "${result_files[@]}")
harness_failed_total=$(jq -s '[.[].summary.harness_failed] | add' "${result_files[@]}")

if (( failed_total > 0 || harness_failed_total > 0 )); then
  printf 'dependent compatibility failures: failed=%s harness_failed=%s\n' \
    "$failed_total" "$harness_failed_total" >&2
  exit 1
fi
