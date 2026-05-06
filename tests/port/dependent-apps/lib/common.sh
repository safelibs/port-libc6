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

dependent_apps_suite_cases() {
  local suite=$1
  jq -r --arg suite "$suite" '.suites[$suite].cases[]?' "$DEPENDENT_APPS_PLAN"
}
