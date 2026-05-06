#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=${ROOT_DIR:-/workspace}
if [[ -n ${SAFE_VERSION:-} ]]; then
  SAFE_VERSION=$SAFE_VERSION
else
  SAFE_VERSION=$(jq -r '.safe_package_version' \
    "$ROOT_DIR/safe/generated/packaging/package-build-manifest.json")
fi
REPO_NAME=safelibs
SCRATCH_REPO=/tmp/safelibs-apt-repo
PACKAGES=(libc6 libc6-dev libc6-dbg libc-bin libc-dev-bin locales nscd)

usage() {
  printf 'Usage: %s <repo-relative-deb-dir>\n' "$0" >&2
  exit 1
}

if [[ $# -ne 1 ]]; then
  usage
fi

DEB_DIR_REL=$1
if [[ "$DEB_DIR_REL" = /* ]]; then
  printf 'safe deb directory must be repository-relative: %s\n' "$DEB_DIR_REL" >&2
  exit 1
fi
if [[ "$DEB_DIR_REL" == *".."* ]]; then
  printf 'safe deb directory must not contain .. segments: %s\n' "$DEB_DIR_REL" >&2
  exit 1
fi

SOURCE_DIR=$ROOT_DIR/$DEB_DIR_REL
if [[ ! -d "$SOURCE_DIR" ]]; then
  printf 'missing deb directory: %s\n' "$SOURCE_DIR" >&2
  exit 1
fi

rm -rf "$SCRATCH_REPO"
mkdir -p "$SCRATCH_REPO/pool" \
  "$SCRATCH_REPO/dists/$REPO_NAME/main/binary-amd64" \
  "$SCRATCH_REPO/dists/$REPO_NAME/main/binary-all"

shopt -s nullglob
debs=("$SOURCE_DIR"/*.deb)
shopt -u nullglob
if (( ${#debs[@]} == 0 )); then
  printf 'no deb files found in %s\n' "$SOURCE_DIR" >&2
  exit 1
fi

cp "${debs[@]}" "$SCRATCH_REPO/pool/"

for deb in "${debs[@]}"; do
  pkg=$(dpkg-deb --field "$deb" Package)
  version=$(dpkg-deb --field "$deb" Version)
  if [[ " ${PACKAGES[*]} " == *" $pkg "* && "$version" != "$SAFE_VERSION" ]]; then
    printf 'unexpected safe package version for %s: %s (expected %s)\n' \
      "$pkg" "$version" "$SAFE_VERSION" >&2
    exit 1
  fi
done

(
  cd "$SCRATCH_REPO"
  dpkg-scanpackages pool /dev/null \
    >"$SCRATCH_REPO/dists/$REPO_NAME/main/binary-amd64/Packages"
)
cp "$SCRATCH_REPO/dists/$REPO_NAME/main/binary-amd64/Packages" \
  "$SCRATCH_REPO/dists/$REPO_NAME/main/binary-all/Packages"
gzip -fk "$SCRATCH_REPO/dists/$REPO_NAME/main/binary-amd64/Packages"
gzip -fk "$SCRATCH_REPO/dists/$REPO_NAME/main/binary-all/Packages"

cat >"$SCRATCH_REPO/dists/$REPO_NAME/Release" <<EOF
Origin: SafeLibs
Label: SafeLibs
Suite: $REPO_NAME
Codename: $REPO_NAME
Architectures: amd64 all
Components: main
Description: SafeLibs local package repository
Date: $(date -Ru)
EOF

cat >/etc/apt/sources.list.d/safelibs-local.list <<EOF
deb [trusted=yes] file:$SCRATCH_REPO $REPO_NAME main
EOF

{
  for pkg in "${PACKAGES[@]}"; do
    cat <<EOF
Package: $pkg
Pin: release o=SafeLibs
Pin-Priority: 1001

EOF
  done
} >/etc/apt/preferences.d/safelibs-local.pref

apt-get update
