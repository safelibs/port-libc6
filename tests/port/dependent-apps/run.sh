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
privileged=0
requested_cases=()

usage() {
  cat <<'USAGE' >&2
Usage: run.sh --image <image:tag> --suite <suite> [--case <case>] [--privileged]

Runs every case in the selected suite by default. Repeat --case to run
specific app cases from that suite. The impacted suite is resolved from
safe/generated/baseline/client-app-regressions.json.
USAGE
}

json_append_case() {
  local cases_json=$1
  local next_json=$2
  local suite_name=$3
  local case_name=$4
  local case_id=$5
  local status=$6
  local failure_kind=$7
  local duration_seconds=$8
  local log_rel=$9
  local safe_version=${10}
  local rerun=${11}
  local case_json

  case_json=$(jq -n \
    --arg suite "$suite_name" \
    --arg case "$case_name" \
    --arg case_id "$case_id" \
    --arg status "$status" \
    --arg log "$log_rel" \
    --arg safe_version "$safe_version" \
    --arg rerun "$rerun" \
    --argjson failure_kind "$failure_kind" \
    --argjson duration_seconds "$duration_seconds" \
    '{
      case_id: $case_id,
      case: $case,
      suite: $suite,
      status: $status,
      failure_kind: $failure_kind,
      duration_seconds: $duration_seconds,
      log: $log,
      safe_version: $safe_version,
      rerun: $rerun
    }')
  jq --argjson case "$case_json" '. + [$case]' "$cases_json" >"$next_json"
  mv "$next_json" "$cases_json"
}

write_result() {
  local result_tmp=$1
  local result_path=$2
  local cases_json=$3
  local result_status=$4
  local total=$5
  local passed=$6
  local failed=$7
  local harness_failed=$8

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
    }' >"$result_tmp"
  mv "$result_tmp" "$result_path"
}

run_impacted_suite() {
  local ledger="$ROOT_DIR/safe/generated/baseline/client-app-regressions.json"
  local result_path="$DEPENDENT_APPS_RESULTS_DIR/impacted.json"
  local suite_log_dir="$DEPENDENT_APPS_LOGS_DIR/impacted"

  ((${#requested_cases[@]} == 0)) || dependent_apps_die 'the impacted suite does not accept --case'
  test -f "$ledger" || dependent_apps_die "missing regression ledger: $ledger"
  jq -e '(.schema_version == 1) and (.issues | type == "array")' "$ledger" >/dev/null ||
    dependent_apps_die "malformed regression ledger: $ledger"
  jq -e 'all(.issues[]; (.impacted_cases | type == "array") and (.impacted_cases | length > 0))' \
    "$ledger" >/dev/null || dependent_apps_die 'non-empty regression issues must list impacted cases'

  rm -rf "$suite_log_dir"
  mkdir -p "$suite_log_dir"

  tmp_dir=$(mktemp -d "$DEPENDENT_APPS_WORK_ROOT/tmp.impacted.XXXXXX")
  cases_json="$tmp_dir/cases.json"
  printf '[]\n' >"$cases_json"
  trap 'rm -rf "$tmp_dir"' EXIT

  mapfile -t impacted_case_ids < <(
    jq -r '[.issues[].impacted_cases[]] | sort | unique | .[]' "$ledger"
  )

  if ((${#impacted_case_ids[@]} == 0)); then
    write_result "$tmp_dir/impacted.json" "$result_path" "$cases_json" passed 0 0 0 0
    return 0
  fi

  [[ -n "$image" ]] || dependent_apps_die '--image is required for non-empty impacted suite'
  dependent_apps_require_command docker
  if ! docker image inspect "$image" >/dev/null 2>&1; then
    dependent_apps_die "missing image or unavailable Docker daemon: $image"
  fi

  passed=0
  failed=0
  harness_failed=0

  for case_id in "${impacted_case_ids[@]}"; do
    [[ "$case_id" == */* ]] || dependent_apps_die "invalid impacted case ID: $case_id"
    original_suite=${case_id%%/*}
    original_case=${case_id#*/}
    [[ "$original_suite" =~ ^[A-Za-z0-9._-]+$ ]] ||
      dependent_apps_die "invalid impacted source suite: $original_suite"
    [[ "$original_case" =~ ^[A-Za-z0-9._-]+$ ]] ||
      dependent_apps_die "invalid impacted source case: $original_case"
    dependent_apps_validate_suite_metadata "$original_suite" ||
      dependent_apps_die "malformed or unknown suite metadata: $original_suite"
    original_suite_type=$(dependent_apps_suite_type "$original_suite")
    if ! dependent_apps_suite_cases "$original_suite" | grep -Fxq "$original_case"; then
      dependent_apps_die "case $original_case is not part of suite $original_suite"
    fi

    case "$original_suite_type" in
      harness-contract)
        ;;
      runtime-smoke)
        case_script=$(dependent_apps_case_script "$original_case")
        [[ -x "$case_script" ]] || dependent_apps_die "missing executable case script: $case_script"
        ;;
      source-build)
        case_script=$(dependent_apps_source_build_script "$original_case")
        [[ -x "$case_script" ]] || dependent_apps_die "missing executable case script: $case_script"
        ;;
      *)
        dependent_apps_die "unsupported suite type for $original_suite: $original_suite_type"
        ;;
    esac

    case_name=${case_id//\//__}
    log_rel="safe/work/dependent-apps/logs/impacted/$case_name.log"
    log_abs="$ROOT_DIR/$log_rel"
    rerun="bash tests/port/dependent-apps/run.sh --image $image --suite impacted"
    if (( privileged )); then
      rerun="$rerun --privileged"
    fi
    started_at=$(date +%s)

    {
      printf 'suite=impacted\n'
      printf 'case=%s\n' "$case_name"
      printf 'case_id=%s\n' "$case_id"
      printf 'safe_version=%s\n' "$safe_version"
      printf 'source_suite=%s\n' "$original_suite"
      printf 'source_case=%s\n' "$original_case"
      printf 'image=%s\n' "$image"
      printf 'rerun=%s\n\n' "$rerun"
    } >"$log_abs"

    if [[ "$original_suite_type" == "harness-contract" ]]; then
      if image_contract_run_case "$image" "$original_case" "$safe_version" >>"$log_abs" 2>&1; then
        status=passed
        failure_kind=null
        passed=$((passed + 1))
      else
        status=harness_failed
        failure_kind='"harness"'
        harness_failed=$((harness_failed + 1))
      fi
    else
      if [[ "$original_suite_type" == "source-build" ]]; then
        container_case_script="/workspace/tests/port/dependent-apps/source-builds/$original_case.sh"
      else
        container_case_script="/workspace/tests/port/dependent-apps/cases/$original_case.sh"
      fi
      case_status_dir="$tmp_dir/status/$case_name"
      rm -rf "$case_status_dir"
      mkdir -p "$case_status_dir"
      container_marker="dependent_apps_container_started=$case_id"
      docker_args=(docker run --rm)
      if (( privileged )); then
        docker_args+=(--privileged)
      fi
      docker_args+=(
        -e "SAFE_VERSION=$safe_version"
        -e "DEPENDENT_APPS_SUITE=impacted"
        -e "DEPENDENT_APPS_CASE=$case_name"
        -e "DEPENDENT_APPS_CASE_ID=$case_id"
        -e "DEPENDENT_APPS_SOURCE_SUITE=$original_suite"
        -e "DEPENDENT_APPS_SOURCE_CASE=$original_case"
        -e "DEPENDENT_APPS_CASE_WORKDIR=/tmp/safelibs-dependent-apps/impacted/$case_name"
      )
      if [[ "$original_suite_type" == "source-build" ]]; then
        docker_args+=(
          -e "DEPENDENT_APPS_FAILURE_KIND_PATH=/tmp/safelibs-dependent-status/failure-kind"
          -v "$case_status_dir:/tmp/safelibs-dependent-status:rw"
        )
      fi
      docker_args+=(
        -v "$HARNESS_DIR:/workspace/tests/port/dependent-apps:ro"
        -w /workspace
        "$image"
        bash -lc 'printf "%s\n" "$1"; shift; exec "$@"' bash "$container_marker" bash "$container_case_script"
      )
      if "${docker_args[@]}" >>"$log_abs" 2>&1; then
        status=passed
        failure_kind=null
        passed=$((passed + 1))
      else
        exit_code=$?
        if grep -Fqx "$container_marker" "$log_abs"; then
          if [[ "$original_suite_type" == "source-build" ]]; then
            source_failure_kind=""
            if [[ -f "$case_status_dir/failure-kind" ]]; then
              source_failure_kind=$(tr -d '\r\n' <"$case_status_dir/failure-kind")
            fi
            printf 'source_build_failure_kind=%s\n' "${source_failure_kind:-unset}" >>"$log_abs"
            case "$source_failure_kind" in
              compatibility_candidate)
                status=failed
                failure_kind='"compatibility_candidate"'
                failed=$((failed + 1))
                ;;
              harness)
                status=harness_failed
                failure_kind='"harness"'
                harness_failed=$((harness_failed + 1))
                ;;
              *)
                printf 'source-build case did not record a valid failure kind for %s with exit code %s\n' \
                  "$case_id" "$exit_code" >>"$log_abs"
                status=harness_failed
                failure_kind='"harness"'
                harness_failed=$((harness_failed + 1))
                ;;
            esac
          else
            status=failed
            failure_kind='"compatibility_candidate"'
            failed=$((failed + 1))
          fi
        else
          printf 'container startup failed for %s with exit code %s\n' "$case_id" "$exit_code" >>"$log_abs"
          status=harness_failed
          failure_kind='"harness"'
          harness_failed=$((harness_failed + 1))
        fi
      fi
    fi

    duration_seconds=$(( $(date +%s) - started_at ))
    json_append_case \
      "$cases_json" "$tmp_dir/cases.next.json" impacted "$case_name" "$case_id" \
      "$status" "$failure_kind" "$duration_seconds" "$log_rel" "$safe_version" "$rerun"
  done

  total=${#impacted_case_ids[@]}
  if (( harness_failed > 0 )); then
    result_status=failed
  elif (( failed > 0 )); then
    result_status=completed_with_compatibility_candidates
  else
    result_status=passed
  fi

  write_result "$tmp_dir/impacted.json" "$result_path" "$cases_json" "$result_status" \
    "$total" "$passed" "$failed" "$harness_failed"

  if (( harness_failed > 0 )); then
    return 1
  fi
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
    --case|--app)
      [[ $# -ge 2 ]] || dependent_apps_die "$1 requires a value"
      requested_cases+=("$2")
      shift 2
      ;;
    --privileged)
      privileged=1
      shift
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

[[ -n "$suite" ]] || dependent_apps_die '--suite is required'
dependent_apps_require_command jq

dependent_apps_prepare_work_dirs
[[ -w "$DEPENDENT_APPS_RESULTS_DIR" ]] || dependent_apps_die "result directory is not writable: $DEPENDENT_APPS_RESULTS_DIR"
[[ -w "$DEPENDENT_APPS_LOGS_DIR" ]] || dependent_apps_die "log directory is not writable: $DEPENDENT_APPS_LOGS_DIR"

safe_version=$(dependent_apps_safe_version)
[[ -n "$safe_version" && "$safe_version" != null ]] || dependent_apps_die 'safe package version is missing'

if [[ "$suite" == "impacted" ]]; then
  run_impacted_suite
  exit $?
fi

[[ -n "$image" ]] || dependent_apps_die '--image is required'
dependent_apps_require_command docker
dependent_apps_validate_suite_metadata "$suite" || dependent_apps_die "malformed or unknown suite metadata: $suite"

if ! docker image inspect "$image" >/dev/null 2>&1; then
  dependent_apps_die "missing image or unavailable Docker daemon: $image"
fi

suite_type=$(dependent_apps_suite_type "$suite")
mapfile -t suite_cases < <(dependent_apps_suite_cases "$suite")

if (( ${#requested_cases[@]} > 0 )); then
  cases=()
  for case_name in "${requested_cases[@]}"; do
    [[ "$case_name" =~ ^[A-Za-z0-9._-]+$ ]] || dependent_apps_die "invalid case name: $case_name"
    if ! printf '%s\n' "${suite_cases[@]}" | grep -Fxq "$case_name"; then
      dependent_apps_die "case $case_name is not part of suite $suite"
    fi
    cases+=("$case_name")
  done
else
  cases=("${suite_cases[@]}")
fi
(( ${#cases[@]} > 0 )) || dependent_apps_die "suite has no selected cases: $suite"

case "$suite_type" in
  harness-contract)
    ;;
  runtime-smoke)
    ;;
  source-build)
    ;;
  *)
    dependent_apps_die "unsupported suite type for $suite: $suite_type"
    ;;
esac

if [[ "$suite_type" != "harness-contract" ]]; then
  for case_name in "${cases[@]}"; do
    if [[ "$suite_type" == "source-build" ]]; then
      case_script=$(dependent_apps_source_build_script "$case_name")
    else
      case_script=$(dependent_apps_case_script "$case_name")
    fi
    [[ -x "$case_script" ]] || dependent_apps_die "missing executable case script: $case_script"
  done
fi

suite_log_dir="$DEPENDENT_APPS_LOGS_DIR/$suite"
case "$suite_log_dir" in
  "$DEPENDENT_APPS_LOGS_DIR/$suite") ;;
  *) dependent_apps_die "refusing unsafe suite log path: $suite_log_dir" ;;
esac
rm -rf "$suite_log_dir"
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
  rerun="bash tests/port/dependent-apps/run.sh --image $image --suite $suite --case $case_name"
  if (( privileged )); then
    rerun="$rerun --privileged"
  fi
  started_at=$(date +%s)

  {
    printf 'suite=%s\n' "$suite"
    printf 'case=%s\n' "$case_name"
    printf 'case_id=%s\n' "$case_id"
    printf 'safe_version=%s\n' "$safe_version"
    printf 'image=%s\n' "$image"
    printf 'rerun=%s\n\n' "$rerun"
  } >"$log_abs"

  if [[ "$suite_type" == "harness-contract" ]]; then
    if image_contract_run_case "$image" "$case_name" "$safe_version" >>"$log_abs" 2>&1; then
      status=passed
      failure_kind=null
      passed=$((passed + 1))
    else
      status=harness_failed
      failure_kind='"harness"'
      harness_failed=$((harness_failed + 1))
    fi
  else
    if [[ "$suite_type" == "source-build" ]]; then
      case_script="/workspace/tests/port/dependent-apps/source-builds/$case_name.sh"
    else
      case_script="/workspace/tests/port/dependent-apps/cases/$case_name.sh"
    fi
    case_status_dir="$tmp_dir/status/$case_name"
    rm -rf "$case_status_dir"
    mkdir -p "$case_status_dir"
    container_marker="dependent_apps_container_started=$case_id"
    docker_args=(docker run --rm)
    if (( privileged )); then
      docker_args+=(--privileged)
    fi
    docker_args+=(
      -e "SAFE_VERSION=$safe_version"
      -e "DEPENDENT_APPS_SUITE=$suite"
      -e "DEPENDENT_APPS_CASE=$case_name"
      -e "DEPENDENT_APPS_CASE_ID=$case_id"
      -e "DEPENDENT_APPS_CASE_WORKDIR=/tmp/safelibs-dependent-apps/$suite/$case_name"
    )
    if [[ "$suite_type" == "source-build" ]]; then
      docker_args+=(
        -e "DEPENDENT_APPS_FAILURE_KIND_PATH=/tmp/safelibs-dependent-status/failure-kind"
        -v "$case_status_dir:/tmp/safelibs-dependent-status:rw"
      )
    fi
    docker_args+=(
      -v "$HARNESS_DIR:/workspace/tests/port/dependent-apps:ro"
      -w /workspace
      "$image"
      bash -lc 'printf "%s\n" "$1"; shift; exec "$@"' bash "$container_marker" bash "$case_script"
    )
    if "${docker_args[@]}" >>"$log_abs" 2>&1; then
      status=passed
      failure_kind=null
      passed=$((passed + 1))
    else
      exit_code=$?
      if grep -Fqx "$container_marker" "$log_abs"; then
        if [[ "$suite_type" == "source-build" ]]; then
          source_failure_kind=""
          if [[ -f "$case_status_dir/failure-kind" ]]; then
            source_failure_kind=$(tr -d '\r\n' <"$case_status_dir/failure-kind")
          fi
          printf 'source_build_failure_kind=%s\n' "${source_failure_kind:-unset}" >>"$log_abs"
          case "$source_failure_kind" in
            compatibility_candidate)
              status=failed
              failure_kind='"compatibility_candidate"'
              failed=$((failed + 1))
              ;;
            harness)
              status=harness_failed
              failure_kind='"harness"'
              harness_failed=$((harness_failed + 1))
              ;;
            *)
              printf 'source-build case did not record a valid failure kind for %s with exit code %s\n' \
                "$case_id" "$exit_code" >>"$log_abs"
              status=harness_failed
              failure_kind='"harness"'
              harness_failed=$((harness_failed + 1))
              ;;
          esac
        else
          status=failed
          failure_kind='"compatibility_candidate"'
          failed=$((failed + 1))
        fi
      else
        printf 'container startup failed for %s with exit code %s\n' "$case_id" "$exit_code" >>"$log_abs"
        status=harness_failed
        failure_kind='"harness"'
        harness_failed=$((harness_failed + 1))
      fi
    fi
  fi

  duration_seconds=$(( $(date +%s) - started_at ))
  json_append_case \
    "$cases_json" "$tmp_dir/cases.next.json" "$suite" "$case_name" "$case_id" \
    "$status" "$failure_kind" "$duration_seconds" "$log_rel" "$safe_version" "$rerun"
done

total=${#cases[@]}
if (( harness_failed > 0 )); then
  result_status=failed
elif (( failed > 0 )); then
  result_status=completed_with_compatibility_candidates
else
  result_status=passed
fi

result_path="$DEPENDENT_APPS_RESULTS_DIR/$suite.json"
result_tmp="$tmp_dir/$suite.json"
write_result "$result_tmp" "$result_path" "$cases_json" "$result_status" \
  "$total" "$passed" "$failed" "$harness_failed"

if (( harness_failed > 0 )); then
  exit 1
fi
