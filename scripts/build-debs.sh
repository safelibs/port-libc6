#!/usr/bin/env bash
# libc6: package via cargo xtask. Expected to fail until the safe
# variant of libc6 is complete.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
. "$repo_root/scripts/lib/build-deb-common.sh"

prepare_rust_env
prepare_dist_dir "$repo_root"

cd "$repo_root/safe"
mkdir -p "$repo_root/dist/debs"
cargo run -p xtask -- package-deb --out "$repo_root/dist/debs"
cp -v "$repo_root/dist/debs"/*.deb "$repo_root/dist"/
