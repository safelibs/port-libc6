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
  local unit_file="$scratch_root/systemd-demo.service"
  local tmpfiles_file="$scratch_root/systemd-demo.tmpfiles"
  local tmpfiles_dir="$scratch_root/systemd-tmpfiles-check"
  rm -rf "$tmpfiles_dir"
  cat >"$unit_file" <<'SYSTEMD_UNIT'
[Unit]
Description=Demo service

[Service]
Type=oneshot
ExecStart=/usr/bin/true

[Install]
WantedBy=multi-user.target
SYSTEMD_UNIT
  cat >"$tmpfiles_file" <<SYSTEMD_TMPFILES
d $tmpfiles_dir 0755 root root - -
SYSTEMD_TMPFILES
  systemd-analyze verify "$unit_file" >/dev/null 2>&1
  systemd-tmpfiles --create "$tmpfiles_file" >/dev/null 2>&1
  [[ -d "$tmpfiles_dir" ]]
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
  local storage_root="$scratch_root/podman-root"
  local run_root="$scratch_root/podman-runroot"
  local output
  rm -rf "$storage_root" "$run_root"
  run_logged podman-pull podman \
    --root "$storage_root" \
    --runroot "$run_root" \
    --storage-driver=vfs \
    pull --tls-verify=false docker.io/library/alpine:3.19
  output=$(podman \
    --root "$storage_root" \
    --runroot "$run_root" \
    --storage-driver=vfs \
    run --cgroups=disabled --rm docker.io/library/alpine:3.19 \
    /bin/sh -c 'printf podman-ok')
  [[ "$output" == "podman-ok" ]]
}

test_gnome_shell() {
  local runtime_dir="$scratch_root/gnome-runtime"
  local gnome_log="$scratch_root/gnome-shell.log"
  local logind_log="$scratch_root/gnome-logind.log"
  local xvfb_log="$scratch_root/gnome-xvfb.log"
  local dbus_pid=""
  local logind_pid=""
  local xvfb_pid=""

  cleanup_gnome_shell() {
    if [[ -n "$xvfb_pid" ]]; then
      kill "$xvfb_pid" >/dev/null 2>&1 || true
      wait "$xvfb_pid" 2>/dev/null || true
    fi
    if [[ -n "$logind_pid" ]]; then
      kill "$logind_pid" >/dev/null 2>&1 || true
      wait "$logind_pid" 2>/dev/null || true
    fi
    if [[ -n "$dbus_pid" ]]; then
      kill "$dbus_pid" >/dev/null 2>&1 || true
      wait "$dbus_pid" 2>/dev/null || true
    fi
    rm -f /run/dbus/pid /tmp/.X99-lock
  }

  trap cleanup_gnome_shell RETURN

  rm -f /run/dbus/pid /tmp/.X99-lock
  mkdir -p /run/dbus "$runtime_dir"
  chmod 700 "$runtime_dir"
  dbus-daemon --system --fork >/dev/null 2>"$scratch_root/gnome-system-dbus.log"
  dbus_pid=$(cat /run/dbus/pid)
  /lib/systemd/systemd-logind >"$logind_log" 2>&1 &
  logind_pid=$!
  Xvfb :99 -screen 0 1024x768x24 >"$xvfb_log" 2>&1 &
  xvfb_pid=$!
  DISPLAY=:99 XDG_RUNTIME_DIR="$runtime_dir" dbus-run-session -- \
    bash -lc 'gnome-shell --x11 --replace >"$1" 2>&1 & shell_pid=$!; sleep 10; kill -0 "$shell_pid"' \
    bash "$gnome_log" >/dev/null 2>&1
  grep -Fq 'GNOME Shell started' "$gnome_log"

  cleanup_gnome_shell
  trap - RETURN
}

test_strace() {
  local strace_bin=${1:-strace}
  local stdout_file="$scratch_root/strace.stdout"
  local trace_file="$scratch_root/strace.out"
  "$strace_bin" -o "$trace_file" -e write bash -lc 'printf traced' >"$stdout_file"
  grep -Fq 'traced' "$stdout_file"
  grep -Fq 'write(1, "traced"' "$trace_file"
}

test_valgrind() {
  local valgrind_bin=${1:-valgrind}
  local valgrind_lib=${2:-}
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
  if [[ -n "$valgrind_lib" ]]; then
    VALGRIND_LIB="$valgrind_lib" \
      "$valgrind_bin" --error-exitcode=1 --leak-check=full "$exe_file" \
      >"$scratch_root/valgrind.log" 2>&1
  else
    "$valgrind_bin" --error-exitcode=1 --leak-check=full "$exe_file" \
      >"$scratch_root/valgrind.log" 2>&1
  fi
}

test_libvirt() {
  local libvirtd_bin=${1:-libvirtd}
  local virt_admin_bin=${2:-virt-admin}
  local lib_path=${3:-}
  local libvirt_root="$scratch_root/libvirt-daemon-check"
  local config_file="$libvirt_root/libvirtd.conf"
  local pid_file="$libvirt_root/libvirtd.pid"
  local daemon_log="$libvirt_root/libvirtd.log"
  local admin_log="$libvirt_root/virt-admin.log"
  local daemon_pid=""

  cleanup_libvirt() {
    if [[ -n "$daemon_pid" ]]; then
      kill "$daemon_pid" >/dev/null 2>&1 || true
      wait "$daemon_pid" 2>/dev/null || true
    fi
  }

  trap cleanup_libvirt RETURN

  getent group libvirt-qemu >/dev/null || groupadd --system libvirt-qemu
  id -u libvirt-qemu >/dev/null 2>&1 || \
    useradd --system --gid libvirt-qemu --home-dir /var/lib/libvirt/qemu --create-home libvirt-qemu

  rm -rf /run/libvirt /var/log/libvirt /var/cache/libvirt "$libvirt_root"
  mkdir -p /run/libvirt /var/log/libvirt /var/cache/libvirt "$libvirt_root"

  cat >"$config_file" <<'LIBVIRT_CONF'
listen_tls = 0
listen_tcp = 0
unix_sock_group = "root"
unix_sock_ro_perms = "0777"
unix_sock_rw_perms = "0770"
auth_unix_ro = "none"
auth_unix_rw = "none"
LIBVIRT_CONF

  if [[ -n "$lib_path" ]]; then
    LD_LIBRARY_PATH="$lib_path" "$libvirtd_bin" \
      -d -t 30 -f "$config_file" -p "$pid_file" >"$daemon_log" 2>&1
  else
    "$libvirtd_bin" -d -t 30 -f "$config_file" -p "$pid_file" >"$daemon_log" 2>&1
  fi

  sleep 3
  daemon_pid=$(cat "$pid_file")

  if [[ -n "$lib_path" ]]; then
    LD_LIBRARY_PATH="$lib_path" "$virt_admin_bin" \
      -c 'virtadm+unix:///system?socket=/run/libvirt/libvirt-admin-sock' \
      srv-list >"$admin_log" 2>&1
  else
    "$virt_admin_bin" \
      -c 'virtadm+unix:///system?socket=/run/libvirt/libvirt-admin-sock' \
      srv-list >"$admin_log" 2>&1
  fi

  grep -Eq '^ 1 +libvirtd$' "$admin_log"

  cleanup_libvirt
  trap - RETURN
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
  local install_root="$package_root/install"
  local libvirt_libdir=""

  rm -rf "$package_root"
  mkdir -p "$package_root" "$install_root"

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

  case "$source_package" in
    strace)
      log "Building source package $source_package"
      run_logged "configure-$source_package" bash -lc \
        "cd '$src_dir' && mkdir -p build && cd build && ../configure --prefix=/usr"
      run_logged "compile-$source_package" bash -lc \
        "cd '$src_dir/build' && make -j$(nproc)"
      run_logged "install-$source_package" bash -lc \
        "cd '$src_dir/build' && make DESTDIR='$install_root' install"
      log "Smoke-testing locally built $dependent_name"
      test_strace "$install_root/usr/bin/strace"
      ;;
    valgrind)
      log "Building source package $source_package"
      run_logged "configure-$source_package" bash -lc \
        "cd '$src_dir' && ./configure --prefix=/usr"
      run_logged "compile-$source_package" bash -lc \
        "cd '$src_dir' && make -j$(nproc)"
      run_logged "install-$source_package" bash -lc \
        "cd '$src_dir' && make DESTDIR='$install_root' install"
      log "Smoke-testing locally built $dependent_name"
      test_valgrind "$install_root/usr/bin/valgrind" "$install_root/usr/libexec/valgrind"
      ;;
    libvirt)
      log "Building source package $source_package"
      run_logged "configure-$source_package" bash -lc \
        "cd '$src_dir' && meson setup build --prefix=/usr"
      run_logged "compile-$source_package" bash -lc \
        "cd '$src_dir' && meson compile -C build"
      run_logged "install-$source_package" bash -lc \
        "cd '$src_dir' && DESTDIR='$install_root' meson install -C build"
      libvirt_libdir=$(dirname "$(find "$install_root/usr/lib" -type f -name 'libvirt.so*' | head -n 1)")
      if [[ -z "$libvirt_libdir" ]]; then
        printf 'Unable to locate locally built libvirt shared libraries under %s.\n' \
          "$install_root" >&2
        return 1
      fi
      log "Smoke-testing locally built $dependent_name"
      test_libvirt "$install_root/usr/sbin/libvirtd" \
        "$install_root/usr/bin/virt-admin" \
        "$libvirt_libdir"
      ;;
    *)
      printf 'No source build rule is defined for source package %s.\n' \
        "$source_package" >&2
      return 1
      ;;
  esac
}

log "Enabling Ubuntu source repositories"
enable_source_repositories

log "Updating package indexes"
run_logged apt-update apt-get update

log "Installing bootstrap tools"
run_logged apt-bootstrap apt-get install -y --no-install-recommends \
  ca-certificates jq build-essential dpkg-dev fakeroot

mapfile -t runtime_packages < <(jq -r '.dependents[].binary_package' "$manifest_path" | sort -u)
helper_packages=(dbus-user-session xvfb libvirt-clients libc6-dbg)
if (( ${#runtime_packages[@]} == 0 )); then
  printf 'No runtime packages were found in %s.\n' "$manifest_path" >&2
  exit 1
fi

log "Installing dependent runtime packages"
run_logged apt-runtime apt-get install -y --no-install-recommends \
  "${runtime_packages[@]}" "${helper_packages[@]}"

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
