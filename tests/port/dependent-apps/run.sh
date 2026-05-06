#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
HARNESS_DIR="$ROOT_DIR/tests/port/dependent-apps"

# shellcheck source=tests/port/dependent-apps/lib/common.sh
. "$HARNESS_DIR/lib/common.sh"
# shellcheck source=tests/port/dependent-apps/lib/image-contract.sh
. "$HARNESS_DIR/lib/image-contract.sh"

image=""
suite=""

usage() {
  cat <<'USAGE' >&2
Usage: run.sh --image <image:tag> --suite <suite>
USAGE
}

while (($#)); do
  case "$1" in
    --image)
      [[ $# -ge 2 ]] || dependent_apps_die '--image requires a value'
      image=$2
      shift 2
      ;;
    --suite)
      [[ $# -ge 2 ]] || dependent_apps_die '--suite requires a value'
      suite=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      dependent_apps_die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$image" ]] || dependent_apps_die '--image is required'
[[ -n "$suite" ]] || dependent_apps_die '--suite is required'
dependent_apps_require_command docker
dependent_apps_require_command jq

dependent_apps_prepare_work_dirs

safe_version=$(dependent_apps_safe_version)
mapfile -t cases < <(dependent_apps_suite_cases "$suite")
(( ${#cases[@]} > 0 )) || dependent_apps_die "suite has no cases: $suite"

case "$suite" in
  image-contract) ;;
  *) dependent_apps_die "unsupported suite for this phase: $suite" ;;
esac

suite_log_dir="$DEPENDENT_APPS_LOGS_DIR/$suite"
mkdir -p "$suite_log_dir"

tmp_dir=$(mktemp -d "$DEPENDENT_APPS_WORK_ROOT/tmp.$suite.XXXXXX")
cases_json="$tmp_dir/cases.json"
printf '[]\n' >"$cases_json"
trap 'rm -rf "$tmp_dir"' EXIT

passed=0
failed=0
harness_failed=0

for case_name in "${cases[@]}"; do
  case_id="$suite/$case_name"
  log_rel="safe/work/dependent-apps/logs/$suite/$case_name.log"
  log_abs="$ROOT_DIR/$log_rel"
  rerun="bash tests/port/dependent-apps/run.sh --image $image --suite $suite"
  started_at=$(date +%s)

  {
    printf 'suite=%s\n' "$suite"
    printf 'case=%s\n' "$case_name"
    printf 'case_id=%s\n' "$case_id"
    printf 'safe_version=%s\n' "$safe_version"
    printf 'image=%s\n' "$image"
    printf 'rerun=%s\n\n' "$rerun"
  } >"$log_abs"

  if image_contract_run_case "$image" "$case_name" "$safe_version" >>"$log_abs" 2>&1; then
    status=passed
    failure_kind=null
    passed=$((passed + 1))
  else
    status=harness_failed
    failure_kind='"harness"'
    harness_failed=$((harness_failed + 1))
  fi

  duration_seconds=$(( $(date +%s) - started_at ))
  case_json=$(jq -n \
    --arg suite "$suite" \
    --arg case "$case_name" \
    --arg case_id "$case_id" \
    --arg status "$status" \
    --arg log "$log_rel" \
    --arg safe_version "$safe_version" \
    --arg rerun "$rerun" \
    --argjson failure_kind "$failure_kind" \
    --argjson duration_seconds "$duration_seconds" \
    '{
      suite: $suite,
      case: $case,
      case_id: $case_id,
      status: $status,
      failure_kind: $failure_kind,
      duration_seconds: $duration_seconds,
      log: $log,
      safe_version: $safe_version,
      rerun: $rerun
    }')
  jq --argjson case "$case_json" '. + [$case]' "$cases_json" >"$tmp_dir/cases.next.json"
  mv "$tmp_dir/cases.next.json" "$cases_json"
done

total=${#cases[@]}
if (( harness_failed == 0 && failed == 0 )); then
  result_status=passed
else
  result_status=failed
fi

result_path="$DEPENDENT_APPS_RESULTS_DIR/$suite.json"
jq -n \
  --arg suite "$suite" \
  --arg status "$result_status" \
  --arg safe_version "$safe_version" \
  --argjson total "$total" \
  --argjson passed "$passed" \
  --argjson failed "$failed" \
  --argjson harness_failed "$harness_failed" \
  --slurpfile cases "$cases_json" \
  '{
    suite: $suite,
    status: $status,
    safe_version: $safe_version,
    summary: {
      total: $total,
      passed: $passed,
      failed: $failed,
      harness_failed: $harness_failed
    },
    cases: $cases[0]
  }' >"$result_path"

if [[ "$result_status" != passed ]]; then
  exit 1
fi
