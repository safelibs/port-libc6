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
  "configure command: cd $SOURCE_BUILD_SRC_DIR && meson setup build --prefix=/usr" \
  bash -lc "cd '$SOURCE_BUILD_SRC_DIR' && meson setup build --prefix=/usr"
source_build_run_compat \
  "build command: cd $SOURCE_BUILD_SRC_DIR && meson compile -C build -j $jobs" \
  bash -lc "cd '$SOURCE_BUILD_SRC_DIR' && meson compile -C build -j $jobs"
source_build_run_compat \
  "install command: cd $SOURCE_BUILD_SRC_DIR && DESTDIR=$SOURCE_BUILD_INSTALL_ROOT meson install -C build" \
  bash -lc "cd '$SOURCE_BUILD_SRC_DIR' && DESTDIR='$SOURCE_BUILD_INSTALL_ROOT' meson install -C build"

libvirt_libdir=$(find "$SOURCE_BUILD_INSTALL_ROOT/usr/lib" -type f -name 'libvirt.so*' -printf '%h\n' | sort -u | head -n 1)
[[ -n "$libvirt_libdir" ]] || \
  source_build_fail compatibility_candidate "unable to locate built libvirt shared libraries under $SOURCE_BUILD_INSTALL_ROOT"
source_build_run_compat \
  "smoke command: locally built libvirt" \
  source_build_smoke_libvirt \
    "$SOURCE_BUILD_INSTALL_ROOT/usr/sbin/libvirtd" \
    "$SOURCE_BUILD_INSTALL_ROOT/usr/bin/virt-admin" \
    "$libvirt_libdir"
