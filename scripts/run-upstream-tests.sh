#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
trace_dir="$repo_root/safe/work/hook-profiles"
trace_path="$trace_dir/run-upstream-tests.json"

requested_profile=${SAFELIBS_UPSTREAM_TEST_PROFILE-}
ci_detected=false
ci_signals=()
if [[ -n "${GITHUB_ACTIONS-}" ]]; then
  ci_detected=true
  ci_signals+=("GITHUB_ACTIONS")
fi
if [[ -n "${SAFELIBS_COMMIT_SHA-}" ]]; then
  ci_detected=true
  ci_signals+=("SAFELIBS_COMMIT_SHA")
fi

if [[ -n "$requested_profile" ]]; then
  effective_profile=$requested_profile
elif $ci_detected; then
  effective_profile=full
else
  effective_profile=legacy
fi

case "$effective_profile" in
  legacy|full) ;;
  *)
    printf 'run-upstream-tests.sh: unsupported SAFELIBS_UPSTREAM_TEST_PROFILE=%s\n' \
      "$effective_profile" >&2
    exit 2
    ;;
esac

executed_commands=()

append_command() {
  executed_commands+=("$*")
}

run_command() {
  append_command "$@"
  "$@"
}

write_trace() {
  mkdir -p "$trace_dir"
  local requested_json ci_signals_json commands_json
  if [[ -n "$requested_profile" ]]; then
    requested_json=$(jq -n --arg value "$requested_profile" '$value')
  else
    requested_json=null
  fi
  ci_signals_json=$(printf '%s\n' "${ci_signals[@]}" | jq -R 'select(length > 0)' | jq -s '.')
  commands_json=$(printf '%s\n' "${executed_commands[@]}" | jq -R 'select(length > 0)' | jq -s '.')
  jq -n \
    --arg script "scripts/run-upstream-tests.sh" \
    --arg effective_profile "$effective_profile" \
    --argjson requested_profile "$requested_json" \
    --argjson ci_detected "$ci_detected" \
    --argjson ci_signals "$ci_signals_json" \
    --argjson executed_commands "$commands_json" \
    '{
      script: $script,
      requested_profile: $requested_profile,
      effective_profile: $effective_profile,
      ci_detected: $ci_detected,
      ci_signals: $ci_signals,
      executed_commands: $executed_commands
    }' >"$trace_path.tmp"
  mv "$trace_path.tmp" "$trace_path"
}

case "$effective_profile" in
  legacy)
    run_command "$repo_root/scripts/run-tests.sh" upstream
    ;;
  full)
    pushd "$repo_root/safe" >/dev/null
    run_command cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    run_command cargo run -p xtask -- build --target amd64 --profile release
    run_command cargo run -p xtask -- check-owned-tests --all-ported \
      --root work/install-root \
      --build-root work/original-build \
      --require-execution-ledger
    popd >/dev/null
    ;;
esac

write_trace
