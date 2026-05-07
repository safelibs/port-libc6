#!/usr/bin/env bash
set -euo pipefail

override_deb_root=/override-debs
status_dir=${VALIDATOR_STATUS_DIR:-/validator/status}

if [[ ! -d "$override_deb_root" ]]; then
  echo "no override packages found; continuing with apt originals"
  exit 0
fi

mapfile -t deb_names < <(find "$override_deb_root" -maxdepth 1 -type f -name '*.deb' -printf '%f\n' | LC_ALL=C sort)
if ((${#deb_names[@]} == 0)); then
  echo "no override packages found; continuing with apt originals"
  exit 0
fi

extract_matching_paths() {
  local deb_path=$1
  shift

  dpkg-deb --fsys-tarfile "$deb_path" |
    tar --overwrite --wildcards -x -f - -C / "$@"
}

echo "installing libc6 validator runtime overrides from $override_deb_root"
for deb_name in "${deb_names[@]}"; do
  deb_path="$override_deb_root/$deb_name"
  package=$(dpkg-deb --field "$deb_path" Package)
  case "$package" in
    libc6)
      extract_matching_paths \
        "$deb_path" \
        "--exclude=./usr/lib64/gconv/*" \
        "--exclude=./usr/lib/x86_64-linux-gnu/gconv/*" \
        "./usr/lib64/*.so*" \
        "./usr/lib/x86_64-linux-gnu/*.so*"
      ;;
    libc-bin)
      extract_matching_paths \
        "$deb_path" \
        "./usr/lib/locale/C.utf8*" \
        "./etc/ld.so.conf" \
        "./etc/ld.so.conf.d/*"
      ;;
    *)
      echo "unsupported fast-install package: $package" >&2
      exit 1
      ;;
  esac
done

mkdir -p "$status_dir"
: >"$status_dir/override-installed"
: >"$status_dir/override-installed-packages.tsv"
for deb_name in "${deb_names[@]}"; do
  deb_path="$override_deb_root/$deb_name"
  package=$(dpkg-deb --field "$deb_path" Package)
  architecture=$(dpkg-deb --field "$deb_path" Architecture)
  version=$(dpkg-deb --field "$deb_path" Version)
  printf '%s\t%s\t%s\t%s\n' "$package" "$version" "$architecture" "$deb_name" >>"$status_dir/override-installed-packages.tsv"
done
