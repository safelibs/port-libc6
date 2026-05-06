#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${ROOT_DIR:-}" ]]; then
  ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)
fi
HARNESS_DIR="${HARNESS_DIR:-$ROOT_DIR/tests/port/dependent-apps}"
# shellcheck source=tests/port/dependent-apps/lib/common.sh
. "$HARNESS_DIR/lib/common.sh"
# shellcheck source=tests/port/dependent-apps/lib/source-build-common.sh
. "$HARNESS_DIR/lib/source-build-common.sh"

case_name="${DEPENDENT_APPS_CASE:-$(basename "${BASH_SOURCE[0]}" .sh)}"
source_build_case_init "$case_name"
source_build_prepare_case

jobs=$(nproc)
source_build_run_compat \
  "configure command: cd $SOURCE_BUILD_SRC_DIR && mkdir -p build && cd build && ../configure --prefix=/usr" \
  bash -lc "cd '$SOURCE_BUILD_SRC_DIR' && mkdir -p build && cd build && ../configure --prefix=/usr"
source_build_run_compat \
  "build command: cd $SOURCE_BUILD_SRC_DIR/build && make -j$jobs" \
  bash -lc "cd '$SOURCE_BUILD_SRC_DIR/build' && make -j$jobs"
source_build_run_compat \
  "install command: cd $SOURCE_BUILD_SRC_DIR/build && make DESTDIR=$SOURCE_BUILD_INSTALL_ROOT install" \
  bash -lc "cd '$SOURCE_BUILD_SRC_DIR/build' && make DESTDIR='$SOURCE_BUILD_INSTALL_ROOT' install"
source_build_run_compat \
  "smoke command: locally built strace" \
  source_build_smoke_strace "$SOURCE_BUILD_INSTALL_ROOT/usr/bin/strace"
