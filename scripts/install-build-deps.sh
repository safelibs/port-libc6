#!/usr/bin/env bash
# Install apt packages and a stable rust toolchain for libc6's safe
# build (cargo xtask). Note: the libc6 safe port is currently
# incomplete; CI is expected to fail at build until the safe variant
# is finished.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential \
  ca-certificates \
  curl \
  file \
  git \
  jq \
  python3 \
  rsync \
  xz-utils

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path

# shellcheck source=/dev/null
. "$HOME/.cargo/env"
rustup default stable
rustc --version
cargo --version

if [[ -n "${GITHUB_PATH:-}" ]]; then
  printf '%s\n' "$HOME/.cargo/bin" >> "$GITHUB_PATH"
fi
