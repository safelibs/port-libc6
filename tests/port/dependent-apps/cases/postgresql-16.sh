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

dependent_apps_require_command runuser
dependent_apps_require_command useradd
dependent_apps_require_command /usr/lib/postgresql/16/bin/initdb
dependent_apps_require_command /usr/lib/postgresql/16/bin/pg_ctl
dependent_apps_require_command /usr/lib/postgresql/16/bin/psql

export LANG=C
export LC_ALL=C

pg_root="$scratch_root/postgresql"
data_dir="$pg_root/data"
socket_dir="$pg_root/socket"
started=0

cleanup_pg() {
  if (( started )); then
    runuser -u pgtest -- /usr/lib/postgresql/16/bin/pg_ctl \
      -D "$data_dir" stop -m fast >/dev/null 2>&1 || true
  fi
}
trap cleanup_pg EXIT

useradd -m -U pgtest >/dev/null 2>&1 || true
install -d -o pgtest -g pgtest "$pg_root" "$data_dir" "$socket_dir"

runuser -u pgtest -- /usr/lib/postgresql/16/bin/initdb --locale=C --encoding=UTF8 -D "$data_dir" \
  >"$pg_root/initdb.log" 2>&1
runuser -u pgtest -- /usr/lib/postgresql/16/bin/pg_ctl \
  -D "$data_dir" -o "-k $socket_dir" -l "$pg_root/server.log" start
started=1
runuser -u pgtest -- /usr/lib/postgresql/16/bin/createdb -h "$socket_dir" smoke

query_output=$(runuser -u pgtest -- /usr/lib/postgresql/16/bin/psql \
  -h "$socket_dir" -d smoke -Atqc 'select 6 * 7')
printf 'postgresql_query_output=%s\n' "$query_output"
[[ "$query_output" == "42" ]]

cleanup_pg
started=0
trap - EXIT
