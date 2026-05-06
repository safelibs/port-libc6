#!/usr/bin/env bash

DEPENDENT_APPS_ROOT_DIR=${ROOT_DIR:?ROOT_DIR must be set before sourcing common.sh}
DEPENDENT_APPS_PLAN="$DEPENDENT_APPS_ROOT_DIR/safe/generated/baseline/dependent-app-test-plan.json"
DEPENDENT_APPS_PACKAGE_MANIFEST="$DEPENDENT_APPS_ROOT_DIR/safe/generated/packaging/package-build-manifest.json"
DEPENDENT_APPS_WORK_ROOT="$DEPENDENT_APPS_ROOT_DIR/safe/work/dependent-apps"
DEPENDENT_APPS_RESULTS_DIR="$DEPENDENT_APPS_WORK_ROOT/results"
DEPENDENT_APPS_LOGS_DIR="$DEPENDENT_APPS_WORK_ROOT/logs"

dependent_apps_die() {
  printf 'dependent-apps: %s\n' "$*" >&2
  exit 1
}

dependent_apps_require_command() {
  local command_name=$1
  command -v "$command_name" >/dev/null 2>&1 || dependent_apps_die "$command_name is required"
}

dependent_apps_prepare_work_dirs() {
  mkdir -p "$DEPENDENT_APPS_WORK_ROOT" "$DEPENDENT_APPS_RESULTS_DIR" "$DEPENDENT_APPS_LOGS_DIR"
}

dependent_apps_safe_version() {
  jq -r '.safe_package_version' "$DEPENDENT_APPS_PACKAGE_MANIFEST"
}

dependent_apps_log() {
  printf '\n==> %s\n' "$*"
}

dependent_apps_suite_cases() {
  local suite=$1
  jq -r --arg suite "$suite" '.suites[$suite].cases[]?' "$DEPENDENT_APPS_PLAN"
}

dependent_apps_suite_type() {
  local suite=$1
  jq -r --arg suite "$suite" '.suites[$suite].type // empty' "$DEPENDENT_APPS_PLAN"
}

dependent_apps_validate_suite_metadata() {
  local suite=$1
  [[ "$suite" =~ ^[A-Za-z0-9._-]+$ ]] || return 1
  jq -e --arg suite "$suite" '
    (.suites | type == "object") and
    (.suites[$suite] | type == "object") and
    (.suites[$suite].type | type == "string") and
    (.suites[$suite].cases | type == "array") and
    (.suites[$suite].cases | length > 0) and
    all(.suites[$suite].cases[]; (type == "string") and test("^[A-Za-z0-9._-]+$")) and
    ((.suites[$suite].cases | length) == (.suites[$suite].cases | unique | length))
  ' "$DEPENDENT_APPS_PLAN" >/dev/null
}

dependent_apps_case_script() {
  local case_name=$1
  printf '%s/tests/port/dependent-apps/cases/%s.sh\n' "$DEPENDENT_APPS_ROOT_DIR" "$case_name"
}

dependent_apps_case_init() {
  local case_name=$1
  export DEPENDENT_APPS_CASE="$case_name"
  export DEPENDENT_APPS_CASE_WORKDIR="${DEPENDENT_APPS_CASE_WORKDIR:-/tmp/safelibs-dependent-apps/$case_name}"
  case "$DEPENDENT_APPS_CASE_WORKDIR" in
    /tmp/*|/var/tmp/*) ;;
    *) dependent_apps_die "refusing unsafe case workdir: $DEPENDENT_APPS_CASE_WORKDIR" ;;
  esac
  rm -rf "$DEPENDENT_APPS_CASE_WORKDIR"
  mkdir -p "$DEPENDENT_APPS_CASE_WORKDIR"
  dependent_apps_log "running $case_name"
}

dependent_apps_case_run() {
  dependent_apps_log "$*"
  "$@"
}
