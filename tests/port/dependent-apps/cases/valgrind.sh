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

dependent_apps_require_command cc
dependent_apps_require_command valgrind
src_file="$scratch_root/valgrind-smoke.c"
exe_file="$scratch_root/valgrind-smoke"
cat >"$src_file" <<'SRC'
#include <stdlib.h>

int main(void) {
  int *p = malloc(sizeof(*p));
  if (p == NULL) {
    return 1;
  }
  *p = 42;
  free(p);
  return 0;
}
SRC

cc -O0 -g "$src_file" -o "$exe_file"
valgrind --error-exitcode=1 --leak-check=full "$exe_file" \
  >"$scratch_root/valgrind.log" 2>&1
grep -F 'ERROR SUMMARY: 0 errors' "$scratch_root/valgrind.log"
