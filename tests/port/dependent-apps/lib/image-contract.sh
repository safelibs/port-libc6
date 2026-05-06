#!/usr/bin/env bash

image_contract_run_case() {
  local image=$1
  local case_name=$2
  local safe_version=$3

  docker run --rm \
    -e "SAFE_VERSION=$safe_version" \
    "$image" \
    bash -s -- "$case_name" <<'CONTRACT_CASE'
set -euo pipefail

case_name=$1
manifest_path=/workspace/dependents.json
plan_path=/workspace/safe/generated/baseline/dependent-app-test-plan.json
safe_version=${SAFE_VERSION:?SAFE_VERSION must be set}
safe_packages=(libc6 libc6-dev libc6-dbg libc-bin libc-dev-bin locales nscd)

require_version() {
  local package=$1
  local version
  version=$(dpkg-query -W -f='${Version}' "$package")
  if [[ "$version" != "$safe_version" ]]; then
    printf '%s version mismatch: got %s, expected %s\n' "$package" "$version" "$safe_version" >&2
    return 1
  fi
}

require_installed() {
  local package=$1
  dpkg-query -W "$package" >/dev/null
}

safe_packages_case() {
  local package
  for package in "${safe_packages[@]}"; do
    require_version "$package"
  done
}

safe_apt_policy_case() {
  local package candidate policy
  for package in "${safe_packages[@]}"; do
    policy=$(apt-cache policy "$package")
    candidate=$(awk '/Candidate:/ {print $2; exit}' <<<"$policy")
    if [[ "$candidate" != "$safe_version" ]]; then
      printf '%s candidate mismatch: got %s, expected %s\n%s\n' \
        "$package" "$candidate" "$safe_version" "$policy" >&2
      return 1
    fi
    if ! grep -Eq 'file:/tmp/safelibs-apt-repo|release .*o=SafeLibs' <<<"$policy"; then
      printf '%s policy does not include the local SafeLibs repository:\n%s\n' \
        "$package" "$policy" >&2
      return 1
    fi
  done
}

dependent_packages_case() {
  local package
  mapfile -t packages < <(jq -r '.dependents[].binary_package' "$manifest_path" | sort -u)
  for package in "${packages[@]}"; do
    require_installed "$package"
  done
}

helper_packages_case() {
  local package command_name
  mapfile -t packages < <(jq -r '.image_helper_packages.runtime[], .image_helper_packages.source_build_bootstrap[]' "$plan_path" | sort -u)
  for package in "${packages[@]}"; do
    require_installed "$package"
  done
  for command_name in Xvfb dbus-run-session dbus-daemon virt-admin jq dpkg-buildpackage fakeroot cc; do
    command -v "$command_name" >/dev/null
  done
}

libc_resolution_case() {
  local command_name binary libc_path owner owner_package owner_version
  local representative_commands=(bash sort python3.12 git ssh ffmpeg strace virt-admin cc)

  for command_name in "${representative_commands[@]}"; do
    binary=$(command -v "$command_name")
    libc_path=$(ldd "$binary" | awk '/libc[.]so[.]6/ {print $3; exit}')
    if [[ -z "$libc_path" ]]; then
      printf 'unable to resolve libc.so.6 for %s (%s)\n' "$command_name" "$binary" >&2
      return 1
    fi
    libc_path=$(readlink -e "$libc_path")
    owner=$(dpkg-query -S "$libc_path" | head -n 1)
    owner_package=${owner%%:*}
    if [[ "$owner_package" != libc6 ]]; then
      printf '%s resolves libc.so.6 to %s owned by %s, expected libc6\n' \
        "$command_name" "$libc_path" "$owner_package" >&2
      return 1
    fi
    owner_version=$(dpkg-query -W -f='${Version}' "$owner_package")
    if [[ "$owner_version" != "$safe_version" ]]; then
      printf '%s owner version mismatch for %s: got %s, expected %s\n' \
        "$command_name" "$libc_path" "$owner_version" "$safe_version" >&2
      return 1
    fi
    printf '%s -> %s (%s %s)\n' "$command_name" "$libc_path" "$owner_package" "$owner_version"
  done
}

case "$case_name" in
  safe-packages)
    safe_packages_case
    ;;
  safe-apt-policy)
    safe_apt_policy_case
    ;;
  dependent-packages)
    dependent_packages_case
    ;;
  helper-packages)
    helper_packages_case
    ;;
  libc-resolution)
    libc_resolution_case
    ;;
  *)
    printf 'unknown image-contract case: %s\n' "$case_name" >&2
    exit 2
    ;;
esac
CONTRACT_CASE
}
