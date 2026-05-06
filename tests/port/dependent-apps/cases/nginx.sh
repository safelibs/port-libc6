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

dependent_apps_require_command nginx
nginx_root="$scratch_root/nginx"
mkdir -p "$nginx_root/conf" "$nginx_root/logs" "$nginx_root/html"
cat >"$nginx_root/conf/nginx.conf" <<'NGINX'
worker_processes 1;
events { worker_connections 16; }
http {
  server {
    listen 8080;
    location / {
      return 200 "ok\n";
    }
  }
}
NGINX

nginx -p "$nginx_root" -c conf/nginx.conf -t
