#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

cd "$ROOT_DIR/safe"
./scripts/stage-original-build.sh --source ../original --build work/original-build
cargo run -p xtask -- package-deb --out work/debs
