#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
MANIFEST_PATH="$ROOT_DIR/dependents.json"
DOCKER_IMAGE="${DOCKER_IMAGE:-ubuntu:24.04}"

if [[ ! -f "$MANIFEST_PATH" ]]; then
  printf 'Missing manifest: %s\n' "$MANIFEST_PATH" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  printf 'docker is required but was not found in PATH.\n' >&2
  exit 1
fi

printf 'Using Docker image %s\n' "$DOCKER_IMAGE"
docker image inspect "$DOCKER_IMAGE" >/dev/null 2>&1 || docker pull "$DOCKER_IMAGE" >/dev/null

# Some package self-tests, especially for strace/valgrind/libvirt, need
# namespace, ptrace, and other kernel features that Docker's default
# confinement blocks.
docker run --rm -i \
  --privileged \
  -e DEBIAN_FRONTEND=noninteractive \
  -v "$ROOT_DIR:/workspace:ro" \
  -w /workspace \
  "$DOCKER_IMAGE" \
  bash -s -- "/workspace/dependents.json" <<'EOF'
set -euo pipefail

manifest_path=$1
scratch_root=/tmp/libc6-dependent-tests
log_dir="$scratch_root/logs"
source_build_root="$scratch_root/source-builds"

mkdir -p "$log_dir" "$source_build_root"

log() {
  printf '\n==> %s\n' "$*"
}

run_logged() {
  local log_name=$1
  shift
  local log_file="$log_dir/${log_name}.log"
  if ! "$@" >"$log_file" 2>&1; then
    printf 'Command failed for %s. Last 200 log lines from %s:\n' "$log_name" "$log_file" >&2
    tail -n 200 "$log_file" >&2 || true
    return 1
  fi
}

enable_source_repositories() {
  cat >/etc/apt/sources.list.d/ubuntu-src.sources <<'SRC'
Types: deb-src
URIs: http://archive.ubuntu.com/ubuntu/
Suites: noble noble-updates noble-backports
Components: main universe restricted multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg

Types: deb-src
URIs: http://security.ubuntu.com/ubuntu/
Suites: noble-security
Components: main universe restricted multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SRC
}

test_bash() {
  local output
  output=$(bash -lc 'printf shell-ok')
  [[ "$output" == "shell-ok" ]]
}

test_coreutils() {
  local output
  output=$(printf 'b\na\n' | env -i PATH=/usr/bin:/bin /usr/bin/sort)
  [[ "$output" == $'a\nb' ]]
}

test_systemd() {
  local output
  output=$(systemd-analyze --version)
  [[ "$output" == *'systemd '* ]]
}

test_python312_minimal() {
  local output
  output=$(python3.12 -c 'print(6 * 7)')
  [[ "$output" == "42" ]]
}

test_git() {
  local repo_dir="$scratch_root/git-smoke"
  rm -rf "$repo_dir"
  git init -b main "$repo_dir" >/dev/null
  git -C "$repo_dir" -c user.name=test -c user.email=test@example.com \
    commit --allow-empty -m init >/dev/null
  git -C "$repo_dir" rev-parse --verify HEAD >/dev/null
}

test_openssh_client() {
  local ssh_dir="$scratch_root/ssh"
  local config_dump="$ssh_dir/ssh-config.txt"
  rm -rf "$ssh_dir"
  mkdir -p "$ssh_dir"
  ssh-keygen -q -t ed25519 -N '' -f "$ssh_dir/testkey" >/dev/null
  ssh -G localhost >"$config_dump" 2>/dev/null
  grep -Fq 'hostname localhost' "$config_dump"
}

test_network_manager() {
  local nm_output="$scratch_root/nmcli-offline.txt"
  nmcli --offline connection add type ethernet con-name test-eth ifname eth0 \
    ipv4.method auto >"$nm_output"
  grep -Fq 'id=test-eth' "$nm_output"
}

test_nginx() {
  local nginx_root="$scratch_root/nginx"
  rm -rf "$nginx_root"
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
  nginx -p "$nginx_root" -c conf/nginx.conf -t >/dev/null 2>&1
}

test_postgresql_16() {
  local pg_root="$scratch_root/postgresql"
  local data_dir="$pg_root/data"
  local socket_dir="$pg_root/socket"
  local started=0

  cleanup_pg() {
    if (( started )); then
      runuser -u pgtest -- /usr/lib/postgresql/16/bin/pg_ctl \
        -D "$data_dir" stop -m fast >/dev/null 2>&1 || true
    fi
  }

  trap cleanup_pg RETURN

  rm -rf "$pg_root"
  useradd -m -U pgtest >/dev/null 2>&1 || true
  install -d -o pgtest -g pgtest "$pg_root" "$data_dir" "$socket_dir"

  runuser -u pgtest -- /usr/lib/postgresql/16/bin/initdb -D "$data_dir" \
    >"$pg_root/initdb.log" 2>&1
  runuser -u pgtest -- /usr/lib/postgresql/16/bin/pg_ctl \
    -D "$data_dir" -o "-k $socket_dir" -l "$pg_root/server.log" start >/dev/null
  started=1
  runuser -u pgtest -- /usr/lib/postgresql/16/bin/createdb -h "$socket_dir" smoke >/dev/null

  local query_output
  query_output=$(runuser -u pgtest -- /usr/lib/postgresql/16/bin/psql \
    -h "$socket_dir" -d smoke -Atqc 'select 6 * 7')
  [[ "$query_output" == "42" ]]

  cleanup_pg
  trap - RETURN
}

test_ffmpeg() {
  ffmpeg -hide_banner -loglevel error \
    -f lavfi -i sine=frequency=1000:duration=1 \
    -f null - >/dev/null
}

test_qemu_system_x86() {
  local pid_file="$scratch_root/qemu.pid"
  rm -f "$pid_file"
  qemu-system-x86_64 -machine none -nodefaults -display none -nographic \
    -monitor none -serial none -accel tcg -daemonize -pidfile "$pid_file"
  kill "$(cat "$pid_file")"
}

test_podman() {
  local output
  output=$(podman --help)
  [[ "$output" == *'Manage pods, containers and images'* ]]
}

test_gnome_shell() {
  local output
  output=$(gnome-shell --version)
  [[ "$output" == *'GNOME Shell '* ]]
}

test_strace() {
  local stdout_file="$scratch_root/strace.stdout"
  local trace_file="$scratch_root/strace.out"
  strace -o "$trace_file" -e write bash -lc 'printf traced' >"$stdout_file"
  grep -Fq 'traced' "$stdout_file"
  grep -Fq 'write(1, "traced"' "$trace_file"
}

test_valgrind() {
  local src_file="$scratch_root/valgrind-smoke.c"
  local exe_file="$scratch_root/valgrind-smoke"
  cat >"$src_file" <<'SRC'
#include <stdlib.h>

int main(void) {
  int *p = malloc(sizeof(*p));
  if (p == NULL) {
    return 1;
  }
  *p = 42;
  free(p);
  return 0;
  }
SRC
  cc "$src_file" -o "$exe_file"
  valgrind --error-exitcode=1 --leak-check=full "$exe_file" \
    >"$scratch_root/valgrind.log" 2>&1
}

test_libvirt() {
  local output
  output=$(libvirtd --version)
  [[ "$output" == *'libvirt'* ]]
}

test_dependent() {
  local name=$1
  case "$name" in
    bash) test_bash ;;
    coreutils) test_coreutils ;;
    systemd) test_systemd ;;
    python3.12-minimal) test_python312_minimal ;;
    git) test_git ;;
    openssh-client) test_openssh_client ;;
    network-manager) test_network_manager ;;
    nginx) test_nginx ;;
    postgresql-16) test_postgresql_16 ;;
    ffmpeg) test_ffmpeg ;;
    qemu-system-x86) test_qemu_system_x86 ;;
    podman) test_podman ;;
    gnome-shell) test_gnome_shell ;;
    strace) test_strace ;;
    valgrind) test_valgrind ;;
    libvirt) test_libvirt ;;
    *)
      printf 'No smoke test is defined for dependent %s.\n' "$name" >&2
      return 1
      ;;
  esac
}

build_source_package() {
  local source_package=$1
  local dependent_name=$2
  local package_root="$source_build_root/$source_package"
  local src_dir
  local -a local_debs=()

  rm -rf "$package_root"
  mkdir -p "$package_root"

  log "Installing build-dependencies for $source_package"
  run_logged "builddep-$source_package" apt-get build-dep -y "$source_package"

  log "Fetching source package $source_package"
  run_logged "source-$source_package" bash -lc "cd '$package_root' && apt-get source '$source_package'"

  src_dir=$(find "$package_root" -mindepth 1 -maxdepth 1 -type d -name "${source_package}-*" | head -n 1)
  if [[ -z "$src_dir" ]]; then
    printf 'Unable to locate unpacked source directory for %s in %s.\n' \
      "$source_package" "$package_root" >&2
    return 1
  fi

  log "Building source package $source_package"
  # The distro package test suites are sensitive to host-kernel behavior that
  # is not reproducible inside Docker. Build the .debs here, install those
  # locally built artifacts, and smoke-test the installed result below.
  run_logged "package-$source_package" bash -lc \
    "cd '$src_dir' && DEB_BUILD_OPTIONS='parallel=$(nproc) nocheck' dpkg-buildpackage -b -uc -us"

  find "$package_root" -maxdepth 1 -type f -name "${source_package}_*.changes" | grep -q .

  case "$source_package" in
    strace)
      mapfile -t local_debs < <(find "$package_root" -maxdepth 1 -type f -name 'strace_*.deb' | sort)
      ;;
    valgrind)
      mapfile -t local_debs < <(find "$package_root" -maxdepth 1 -type f -name 'valgrind_*.deb' | sort)
      ;;
    libvirt)
      mapfile -t local_debs < <(
        find "$package_root" -maxdepth 1 -type f \
          \( -name 'libvirt0_*.deb' -o -name 'libvirt-daemon_*.deb' \) | sort
      )
      ;;
    *)
      printf 'No built package installation rule is defined for source package %s.\n' \
        "$source_package" >&2
      return 1
      ;;
  esac

  if (( ${#local_debs[@]} == 0 )); then
    printf 'No local .deb artifacts were found for %s in %s.\n' \
      "$source_package" "$package_root" >&2
    return 1
  fi

  log "Installing locally built packages for $source_package"
  run_logged "install-$source_package" apt-get install -y --allow-downgrades \
    --no-install-recommends "${local_debs[@]}"

  log "Smoke-testing locally built $dependent_name"
  test_dependent "$dependent_name"
}

log "Enabling Ubuntu source repositories"
enable_source_repositories

log "Updating package indexes"
run_logged apt-update apt-get update

log "Installing bootstrap tools"
run_logged apt-bootstrap apt-get install -y --no-install-recommends \
  ca-certificates jq build-essential dpkg-dev fakeroot

mapfile -t runtime_packages < <(jq -r '.dependents[].binary_package' "$manifest_path" | sort -u)
if (( ${#runtime_packages[@]} == 0 )); then
  printf 'No runtime packages were found in %s.\n' "$manifest_path" >&2
  exit 1
fi

log "Installing dependent runtime packages"
run_logged apt-runtime apt-get install -y --no-install-recommends "${runtime_packages[@]}"

declare -A built_sources=()
mapfile -t dependents < <(jq -c '.dependents[]' "$manifest_path")

for dependent_json in "${dependents[@]}"; do
  name=$(jq -r '.name' <<<"$dependent_json")
  binary_package=$(jq -r '.binary_package' <<<"$dependent_json")
  log "Smoke-testing $name ($binary_package)"
  test_dependent "$name"

  if jq -e 'any(.dependency_modes[]?; . == "compile_time_via_glibc_dev")' \
    >/dev/null <<<"$dependent_json"; then
    source_package=$(jq -r '.source_package' <<<"$dependent_json")
    if [[ -z "${built_sources[$source_package]+set}" ]]; then
      build_source_package "$source_package" "$name"
      built_sources["$source_package"]=1
    fi
  fi
done

log "All dependent package checks passed"
EOF
