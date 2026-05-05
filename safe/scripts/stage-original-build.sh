#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

usage() {
  printf 'Usage: %s --source <upstream-source-dir> --build <build-dir>\n' "$0" >&2
  exit 1
}

resolve_path() {
  local path=$1
  if [[ "$path" = /* ]]; then
    printf '%s\n' "$path"
  else
    printf '%s\n' "$ROOT_DIR/$path"
  fi
}

validate_build_tree() {
  local build_dir=$1
  [[ -f "$build_dir/testroot.pristine/install.stamp" ]] || return 1
  [[ -x "$build_dir/elf/ld-linux-x86-64.so.2" ]] || return 1
  [[ -f "$build_dir/testrun.sh" ]] || return 1
  [[ -d "$build_dir/iconvdata" ]] || return 1
  [[ -d "$build_dir/localedata" ]] || return 1
  return 0
}

SOURCE_ARG=
BUILD_ARG=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      [[ $# -ge 2 ]] || usage
      SOURCE_ARG=$2
      shift 2
      ;;
    --build)
      [[ $# -ge 2 ]] || usage
      BUILD_ARG=$2
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[[ -n "$SOURCE_ARG" && -n "$BUILD_ARG" ]] || usage

SOURCE_DIR=$(resolve_path "$SOURCE_ARG")
BUILD_DIR=$(resolve_path "$BUILD_ARG")
INSTALL_STAMP=$BUILD_DIR/testroot.pristine/install.stamp

if [[ ! -d "$SOURCE_DIR" ]]; then
  printf 'missing upstream source directory: %s\n' "$SOURCE_DIR" >&2
  exit 1
fi
if [[ ! -x "$SOURCE_DIR/configure" ]]; then
  printf 'missing configure script under upstream source: %s\n' "$SOURCE_DIR/configure" >&2
  exit 1
fi

if validate_build_tree "$BUILD_DIR"; then
  exit 0
fi

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

cat >"$BUILD_DIR/configparms" <<'EOF'
bindir=/usr/bin
rootsbindir=/usr/sbin
sbindir=/usr/sbin
libdir=/usr/lib64
slibdir=/usr/lib64
rtlddir=/usr/lib64
libexecdir=/usr/libexec
includedir=/usr/include
complocaledir=/usr/lib/locale
localedir=/usr/share/locale
i18ndir=/usr/share/i18n
vardbdir=/var/db
EOF

(
  cd "$BUILD_DIR"
  "$SOURCE_DIR/configure" \
    --prefix=/usr \
    --disable-werror \
    --disable-crypt \
    --without-selinux \
    --enable-bind-now \
    --enable-fortify-source \
    --enable-stack-protector=strong \
    --with-timeoutfactor=25
  make -j"$(nproc)"
)

mkdir -p "$BUILD_DIR/testroot.pristine/usr/include/gnu"
cp "$BUILD_DIR/gnu/lib-names-64.h" "$BUILD_DIR/testroot.pristine/usr/include/gnu/lib-names-64.h"
cp "$BUILD_DIR/gnu/lib-names.h" "$BUILD_DIR/testroot.pristine/usr/include/gnu/lib-names.h"

(
  cd "$BUILD_DIR"
  make testroot.pristine/install.stamp
)
