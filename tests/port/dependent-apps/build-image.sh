#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
HARNESS_DIR="$ROOT_DIR/tests/port/dependent-apps"
WORK_ROOT="$ROOT_DIR/safe/work/dependent-apps"
IMAGE_CONTEXT="$WORK_ROOT/image-context"
RESULTS_DIR="$WORK_ROOT/results"
LOGS_DIR="$WORK_ROOT/logs"
PLAN_PATH="$ROOT_DIR/safe/generated/baseline/dependent-app-test-plan.json"
PACKAGE_MANIFEST="$ROOT_DIR/safe/generated/packaging/package-build-manifest.json"
INSTALL_SAFE_REPO="$ROOT_DIR/safe/scripts/install-safe-repo.sh"

debs_path=""
manifest_path="$ROOT_DIR/dependents.json"
image_tag=""

usage() {
  cat <<'USAGE' >&2
Usage: build-image.sh --debs <deb-dir> --manifest <dependents.json> [--tag <image:tag>]

Builds the dependent application contract image from existing SafeLibs .deb
artifacts. When --tag is omitted, the tag is derived from the safe package
version with non Docker-tag characters replaced by '-'.
USAGE
}

die() {
  printf 'build-image.sh: %s\n' "$*" >&2
  exit 1
}

sanitize_tag_component() {
  printf '%s' "$1" | sed 's/[^A-Za-z0-9_.-]/-/g'
}

absolute_path() {
  local path=$1
  if [[ "$path" = /* ]]; then
    printf '%s\n' "$path"
  else
    printf '%s\n' "$ROOT_DIR/$path"
  fi
}

while (($#)); do
  case "$1" in
    --debs)
      [[ $# -ge 2 ]] || die '--debs requires a value'
      debs_path=$(absolute_path "$2")
      shift 2
      ;;
    --manifest)
      [[ $# -ge 2 ]] || die '--manifest requires a value'
      manifest_path=$(absolute_path "$2")
      shift 2
      ;;
    --tag)
      [[ $# -ge 2 ]] || die '--tag requires a value'
      image_tag=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$debs_path" ]] || die '--debs is required'
[[ -d "$debs_path" ]] || die "missing deb directory: $debs_path"
[[ -f "$manifest_path" ]] || die "missing dependent manifest: $manifest_path"
[[ -f "$PLAN_PATH" ]] || die "missing dependent app test plan: $PLAN_PATH"
[[ -f "$PACKAGE_MANIFEST" ]] || die "missing package build manifest: $PACKAGE_MANIFEST"
[[ -f "$INSTALL_SAFE_REPO" ]] || die "missing safe repository installer: $INSTALL_SAFE_REPO"
command -v docker >/dev/null 2>&1 || die 'docker is required'
command -v jq >/dev/null 2>&1 || die 'jq is required'

safe_version=$(jq -r '.safe_package_version' "$PACKAGE_MANIFEST")
[[ -n "$safe_version" && "$safe_version" != null ]] || die 'package manifest has no safe_package_version'

dependent_count=$(jq -r '[.dependents[].binary_package] | unique | length' "$manifest_path")
[[ "$dependent_count" =~ ^[0-9]+$ && "$dependent_count" -gt 0 ]] || die 'dependent manifest has no binary packages'

if [[ -z "$image_tag" ]]; then
  image_tag="safelibs-libc6-dependent:$(sanitize_tag_component "$safe_version")"
fi

shopt -s nullglob
debs=("$debs_path"/*.deb)
shopt -u nullglob
(( ${#debs[@]} > 0 )) || die "no .deb files found in $debs_path"

case "$IMAGE_CONTEXT" in
  "$ROOT_DIR/safe/work/dependent-apps/image-context") ;;
  *) die "refusing to refresh unexpected image context path: $IMAGE_CONTEXT" ;;
esac

mkdir -p "$WORK_ROOT" "$RESULTS_DIR" "$LOGS_DIR"
rm -rf "$IMAGE_CONTEXT"
mkdir -p \
  "$IMAGE_CONTEXT/debs" \
  "$IMAGE_CONTEXT/safe/scripts" \
  "$IMAGE_CONTEXT/safe/generated/packaging" \
  "$IMAGE_CONTEXT/safe/generated/baseline"

cp -p "$HARNESS_DIR/Dockerfile" "$IMAGE_CONTEXT/Dockerfile"
cp -p "${debs[@]}" "$IMAGE_CONTEXT/debs/"
cp -p "$manifest_path" "$IMAGE_CONTEXT/dependents.json"
cp -p "$PLAN_PATH" "$IMAGE_CONTEXT/safe/generated/baseline/dependent-app-test-plan.json"
cp -p "$PACKAGE_MANIFEST" "$IMAGE_CONTEXT/safe/generated/packaging/package-build-manifest.json"
cp -p "$INSTALL_SAFE_REPO" "$IMAGE_CONTEXT/safe/scripts/install-safe-repo.sh"

docker build \
  --build-arg "SAFE_VERSION=$safe_version" \
  --build-arg "DEPENDENT_COUNT=$dependent_count" \
  --tag "$image_tag" \
  "$IMAGE_CONTEXT"

printf '%s\n' "$image_tag"
