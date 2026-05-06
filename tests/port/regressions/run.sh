#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
LEDGER="$ROOT_DIR/safe/generated/baseline/client-app-regressions.json"
RESULT_DIR="$ROOT_DIR/safe/work/regressions"
RESULT_PATH="$RESULT_DIR/results.json"
MANIFEST="$ROOT_DIR/safe/tests/manifest.toml"

die() {
  printf 'regressions: %s\n' "$*" >&2
  exit 1
}

require_command() {
  local command_name=$1
  command -v "$command_name" >/dev/null 2>&1 || die "$command_name is required"
}

shell_command_string() {
  local rendered=""
  local word
  for word in "$@"; do
    rendered+=" $(printf '%q' "$word")"
  done
  printf '%s\n' "${rendered# }"
}

json_append_result() {
  local results_json=$1
  local next_json=$2
  local issue_id=$3
  local reproducer=$4
  local runner_kind=$5
  local status=$6
  local duration_seconds=$7
  local executed_command=$8
  local result_json

  result_json=$(jq -n \
    --arg issue_id "$issue_id" \
    --arg reproducer "$reproducer" \
    --arg runner_kind "$runner_kind" \
    --arg status "$status" \
    --arg executed_command "$executed_command" \
    --argjson duration_seconds "$duration_seconds" \
    '{
      issue_id: $issue_id,
      reproducer: $reproducer,
      runner_kind: $runner_kind,
      status: $status,
      duration_seconds: $duration_seconds,
      executed_command: $executed_command
    }')
  jq --argjson result "$result_json" '. + [$result]' "$results_json" >"$next_json"
  mv "$next_json" "$results_json"
}

resolve_safe_test_catalog_ids() {
  local reproducer=$1
  local output=$2

  python3 - "$MANIFEST" "$reproducer" >"$output" <<'PY'
import sys
import tomllib

manifest_path, reproducer = sys.argv[1], sys.argv[2]
with open(manifest_path, "rb") as handle:
    manifest = tomllib.load(handle)

catalog_ids = []
for entry in manifest.get("entries", []):
    if entry.get("safe_path") == reproducer or reproducer in entry.get("support_paths", []):
        catalog_ids.append(entry["catalog_id"])

if not catalog_ids:
    sys.exit(2)

for catalog_id in sorted(set(catalog_ids)):
    print(catalog_id)
PY
}

write_results() {
  local results_json=$1
  local result_tmp=$2
  local total=$3
  local passed=$4
  local failed=$5

  jq -n \
    --argjson total "$total" \
    --argjson passed "$passed" \
    --argjson failed "$failed" \
    --slurpfile results "$results_json" \
    '{
      schema_version: 1,
      summary: {
        total: $total,
        passed: $passed,
        failed: $failed
      },
      results: $results[0]
    }' >"$result_tmp"
  mv "$result_tmp" "$RESULT_PATH"
}

require_command jq
require_command python3
test -f "$LEDGER" || die "missing regression ledger: $LEDGER"
jq -e '(.schema_version == 1) and (.issues | type == "array")' "$LEDGER" >/dev/null ||
  die "malformed regression ledger: $LEDGER"

mkdir -p "$RESULT_DIR"
tmp_dir=$(mktemp -d "$RESULT_DIR/tmp.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT
results_json="$tmp_dir/results.json"
printf '[]\n' >"$results_json"

mapfile -t issues < <(jq -c '.issues[]' "$LEDGER")
passed=0
failed=0

for issue_json in "${issues[@]}"; do
  issue_id=$(jq -r '.id' <<<"$issue_json")
  reproducer=$(jq -r '.reproducer' <<<"$issue_json")
  started_at=$(date +%s)
  status=failed
  runner_kind=""
  executed_command=""

  case "$reproducer" in
    tests/port/regressions/*.sh)
      runner_kind=port-regression
      test -x "$ROOT_DIR/$reproducer" || die "missing executable reproducer: $reproducer"
      executed_command="cd $(printf '%q' "$ROOT_DIR") && $(printf '%q' "$reproducer")"
      if (cd "$ROOT_DIR" && "$reproducer"); then
        status=passed
      fi
      ;;
    safe/tests/*)
      runner_kind=safe-tests
      test -f "$ROOT_DIR/$reproducer" || die "missing safe test reproducer: $reproducer"
      catalog_ids_path="$tmp_dir/catalog.$issue_id"
      if ! resolve_safe_test_catalog_ids "$reproducer" "$catalog_ids_path"; then
        die "no safe/tests/manifest.toml entry maps to $reproducer"
      fi
      mapfile -t catalog_ids <"$catalog_ids_path"
      ((${#catalog_ids[@]} > 0)) || die "no catalog IDs resolved for $reproducer"
      cmd=(cargo run -p xtask -- run-original-tests --root work/install-root --build-root work/original-build)
      for catalog_id in "${catalog_ids[@]}"; do
        cmd+=(--tests "$catalog_id")
      done
      cmd+=(--privileged-container-tests)
      executed_command="cd $(printf '%q' "$ROOT_DIR/safe") && $(shell_command_string "${cmd[@]}")"
      if (cd "$ROOT_DIR/safe" && "${cmd[@]}"); then
        status=passed
      fi
      ;;
    *)
      die "unsupported reproducer path for $issue_id: $reproducer"
      ;;
  esac

  duration_seconds=$(( $(date +%s) - started_at ))
  if [[ "$status" == passed ]]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
  fi
  json_append_result \
    "$results_json" "$tmp_dir/results.next.json" "$issue_id" "$reproducer" \
    "$runner_kind" "$status" "$duration_seconds" "$executed_command"
done

total=${#issues[@]}
write_results "$results_json" "$tmp_dir/results.final.json" "$total" "$passed" "$failed"

if (( failed > 0 )); then
  exit 1
fi
