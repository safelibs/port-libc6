#!/usr/bin/env bash
# libc6: package via cargo xtask. Expected to fail until the safe
# variant of libc6 is complete.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="$repo_root/dist"

# shellcheck source=/dev/null
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

if [[ -d "$HOME/.cargo/bin" ]]; then
  case ":$PATH:" in
    *":$HOME/.cargo/bin:"*) ;;
    *) export PATH="$HOME/.cargo/bin:$PATH" ;;
  esac
fi

rm -rf -- "$dist_dir"
mkdir -p -- "$dist_dir/debs"

cd "$repo_root/safe"
cargo run -p xtask -- package-deb --out "$dist_dir/debs"
cp -v "$dist_dir"/debs/*.deb "$dist_dir"/
