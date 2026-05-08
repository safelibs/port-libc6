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

rewrite_control_field() {
  local control_file=$1
  local field=$2
  local value=$3
  local tmp
  tmp="$(mktemp)"
  awk -v field="$field" -v value="$value" '
    BEGIN { replaced = 0; skipping = 0 }
    /^[^[:space:]:][^:]*:/ {
      current = substr($0, 1, index($0, ":") - 1)
      if (current == field) {
        if (!replaced) {
          print field ": " value
          replaced = 1
        }
        skipping = 1
        next
      }
      skipping = 0
    }
    /^[[:space:]]/ && skipping { next }
    { print }
    END {
      if (!replaced) {
        print field ": " value
      }
    }
  ' "$control_file" >"$tmp"
  mv "$tmp" "$control_file"
}

patch_locales_postinst() {
  local postinst=$1
  python3 - "$postinst" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
old = """    # Update requested locales if locales-all is not installed
    if [ "$(dpkg-query -W -f='${db:Status-Want}' locales-all 2>/dev/null)" = 'install' ] ; then
        echo "locales-all installed, skipping locales generation"
    else
        locale-gen
    fi
"""
new = """    # Update requested locales if locales-all is not installed
    if [ "$(dpkg-query -W -f='${db:Status-Want}' locales-all 2>/dev/null)" = 'install' ] ; then
        echo "locales-all installed, skipping locales generation"
    elif [ -e "$LG" ] && grep -E -q '^[[:space:]]*[A-Za-z0-9_.@-]+[[:space:]]+[A-Za-z0-9_-]+[[:space:]]*$' "$LG"; then
        locale-gen
    else
        echo "No locales selected, skipping locales generation"
    fi
"""
if old in text:
    path.write_text(text.replace(old, new))
PY
}

stage_builtin_c_utf8_locale() {
  local package_root=$1
  local output="$package_root/usr/lib/locale/C.utf8"
  if [[ -f "$output/LC_CTYPE" && -f "$output/LC_COLLATE" ]]; then
    return 0
  fi
  mkdir -p "$package_root/usr/lib/locale"
  rm -rf "$output"
  localedef --no-archive --prefix="$package_root" -i C -f UTF-8 C.UTF-8
  for required in \
    LC_CTYPE \
    LC_COLLATE \
    LC_TIME \
    LC_MESSAGES/SYS_LC_MESSAGES
  do
    [[ -f "$output/$required" ]] || {
      printf 'run-port-tests.sh: generated C.UTF-8 locale missing %s\n' "$required" >&2
      return 1
    }
  done
}

prepare_validator_dist_debs() {
  local dist_dir="$repo_root/dist"
  if ! compgen -G "$dist_dir/*.deb" >/dev/null; then
    return 0
  fi
  local work_root="$repo_root/safe/work/validator-dist-debs"
  rm -rf "$work_root"
  mkdir -p "$work_root"
  local deb package root rebuilt
  for deb in "$dist_dir"/*.deb; do
    package="$(dpkg-deb --field "$deb" Package)"
    root="$work_root/${package}.root"
    rebuilt="$work_root/$(basename "$deb")"
    rm -rf "$root"
    mkdir -p "$root"
    dpkg-deb -R "$deb" "$root"
    case "$package" in
      nscd)
        rewrite_control_field \
          "$root/DEBIAN/control" \
          "Pre-Depends" \
          "init-system-helpers (>= 1.54~)"
        ;;
      libc-bin)
        stage_builtin_c_utf8_locale "$root"
        ;;
      locales)
        if [[ -f "$root/DEBIAN/postinst" ]]; then
          patch_locales_postinst "$root/DEBIAN/postinst"
        fi
        ;;
    esac
    dpkg-deb --build --root-owner-group -Zgzip -z1 "$root" "$rebuilt" >/dev/null
    mv "$rebuilt" "$deb"
  done
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

prepare_validator_dist_debs
write_trace
