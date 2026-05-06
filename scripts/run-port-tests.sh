#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
trace_dir="$repo_root/safe/work/hook-profiles"
trace_path="$trace_dir/run-port-tests.json"

requested_profile=${SAFELIBS_PORT_TEST_PROFILE-}
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
  effective_profile=quick
fi

case "$effective_profile" in
  quick|full) ;;
  *)
    printf 'run-port-tests.sh: unsupported SAFELIBS_PORT_TEST_PROFILE=%s\n' \
      "$effective_profile" >&2
    exit 2
    ;;
esac

executed_commands=()
executed_suites=()

append_command() {
  executed_commands+=("$*")
}

run_command() {
  append_command "$@"
  "$@"
}

record_suite() {
  executed_suites+=("$1")
}

write_trace() {
  mkdir -p "$trace_dir"
  local requested_json ci_signals_json commands_json suites_json
  if [[ -n "$requested_profile" ]]; then
    requested_json=$(jq -n --arg value "$requested_profile" '$value')
  else
    requested_json=null
  fi
  ci_signals_json=$(printf '%s\n' "${ci_signals[@]}" | jq -R 'select(length > 0)' | jq -s '.')
  commands_json=$(printf '%s\n' "${executed_commands[@]}" | jq -R 'select(length > 0)' | jq -s '.')
  suites_json=$(printf '%s\n' "${executed_suites[@]}" | jq -R 'select(length > 0)' | jq -s '.')
  jq -n \
    --arg script "scripts/run-port-tests.sh" \
    --arg effective_profile "$effective_profile" \
    --argjson requested_profile "$requested_json" \
    --argjson ci_detected "$ci_detected" \
    --argjson ci_signals "$ci_signals_json" \
    --argjson executed_commands "$commands_json" \
    --argjson executed_suites "$suites_json" \
    '{
      script: $script,
      requested_profile: $requested_profile,
      effective_profile: $effective_profile,
      ci_detected: $ci_detected,
      ci_signals: $ci_signals,
      executed_commands: $executed_commands,
      executed_suites: $executed_suites
    }' >"$trace_path.tmp"
  mv "$trace_path.tmp" "$trace_path"
}

select_deb_dir() {
  for candidate in \
    "$repo_root/safe/work/debs" \
    "$repo_root/dist/debs" \
    "$repo_root/dist"
  do
    if compgen -G "$candidate/*.deb" >/dev/null; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  printf 'run-port-tests.sh: no .deb artifacts found; run scripts/build-debs.sh first\n' >&2
  return 1
}

safe_version="$(jq -r '.safe_package_version' "$repo_root/safe/generated/packaging/package-build-manifest.json")"
safe_image_tag="$(printf '%s' "$safe_version" | sed 's/[^A-Za-z0-9_.-]/-/g')"
image="${SAFELIBS_DEPENDENT_APP_IMAGE:-safelibs-libc6-dependent:${safe_image_tag}}"
deb_dir="$(select_deb_dir)"

run_command bash "$repo_root/tests/port/dependent-apps/build-image.sh" \
  --debs "$deb_dir" \
  --manifest "$repo_root/dependents.json" \
  --tag "$image"

run_suite() {
  local suite=$1
  shift
  run_command bash "$repo_root/tests/port/dependent-apps/run.sh" --image "$image" --suite "$suite" "$@"
  record_suite "$suite"
}

case "$effective_profile" in
  quick)
    run_suite image-contract
    run_suite core-network
    ;;
  full)
    run_suite image-contract
    run_suite all-runtime --privileged
    run_suite source-builds --privileged
    ;;
esac

if [[ -x "$repo_root/tests/port/regressions/run.sh" ]]; then
  run_command bash "$repo_root/tests/port/regressions/run.sh"
  record_suite regressions
fi

write_trace
