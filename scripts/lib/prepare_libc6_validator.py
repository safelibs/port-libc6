#!/usr/bin/env python3
"""Prepare a libc6 validator checkout with override debs preinstalled.

The upstream validator installs override debs inside every testcase container.
That is acceptable for small libraries, but libc6 replaces the base runtime
package set and dpkg can consume most of the shorter 60 second usage-test
timeout before the testcase starts. This helper copies the validator checkout,
adds the current libc6 override debs to the Docker build context, and patches
the copied install hook so each testcase records the already-installed package
set instead of reinstalling it.
"""

from __future__ import annotations

import shutil
import stat
import sys
from pathlib import Path


INSTALL_OVERRIDE_DEBS = """#!/usr/bin/env bash
set -euo pipefail

override_deb_root=/override-debs
status_dir=${VALIDATOR_STATUS_DIR:-/validator/status}

if [[ ! -d "$override_deb_root" ]]; then
  echo "no override packages found; continuing with apt originals"
  exit 0
fi

mapfile -t deb_names < <(find "$override_deb_root" -maxdepth 1 -type f -name '*.deb' -printf '%f\\n' | LC_ALL=C sort)
if ((${#deb_names[@]} == 0)); then
  echo "no override packages found; continuing with apt originals"
  exit 0
fi

write_status_records() {
  mkdir -p "$status_dir"
  : >"$status_dir/override-installed"
  : >"$status_dir/override-installed-packages.tsv"
  for deb_name in "${deb_names[@]}"; do
    deb_path="$override_deb_root/$deb_name"
    package=$(dpkg-deb --field "$deb_path" Package)
    architecture=$(dpkg-deb --field "$deb_path" Architecture)
    version=$(dpkg-query -W -f='${Version}' "$package")
    printf '%s\\t%s\\t%s\\t%s\\n' "$package" "$version" "$architecture" "$deb_name" >>"$status_dir/override-installed-packages.tsv"
  done
}

all_preinstalled=1
for deb_name in "${deb_names[@]}"; do
  deb_path="$override_deb_root/$deb_name"
  package=$(dpkg-deb --field "$deb_path" Package)
  version=$(dpkg-deb --field "$deb_path" Version)
  installed=$(dpkg-query -W -f='${Version}' "$package" 2>/dev/null || true)
  if [[ "$installed" != "$version" ]]; then
    all_preinstalled=0
    break
  fi
done

if ((all_preinstalled)); then
  echo "override packages already installed in validator image"
  write_status_records
  exit 0
fi

debs=()
for deb_name in "${deb_names[@]}"; do
  debs+=("$override_deb_root/$deb_name")
done

echo "installing override packages from $override_deb_root"
dpkg --install --force-downgrade "${debs[@]}"
write_status_records
"""


def fail(message: str) -> "None":
    print(f"prepare_libc6_validator: {message}", file=sys.stderr)
    sys.exit(1)


def copy_validator(source: Path, dest: Path) -> None:
    if not (source / "test.sh").is_file():
        fail(f"source validator checkout missing test.sh: {source}")
    if dest.exists():
        shutil.rmtree(dest)
    ignore = shutil.ignore_patterns(".git", "__pycache__", ".pytest_cache")
    shutil.copytree(source, dest, symlinks=True, ignore=ignore)


def copy_override_debs(override_deb_dir: Path, dest: Path) -> None:
    debs = sorted(override_deb_dir.glob("*.deb"))
    if not debs:
        fail(f"no override debs found under {override_deb_dir}")
    dest.mkdir(parents=True, exist_ok=True)
    for stale in dest.glob("*.deb"):
        stale.unlink()
    for deb in debs:
        shutil.copy2(deb, dest / deb.name)


def patch_dockerfile(path: Path) -> None:
    text = path.read_text()
    marker = "COPY libc6/ /validator/tests/libc6/\n"
    install_step = (
        "RUN set -eux; \\\n"
        "    dpkg --install --force-downgrade /validator/tests/libc6/override-debs/*.deb\n"
    )
    if install_step in text:
        return
    if marker not in text:
        fail(f"could not find libc6 COPY marker in {path}")
    path.write_text(text.replace(marker, marker + "\n" + install_step, 1))


def patch_install_hook(path: Path) -> None:
    path.write_text(INSTALL_OVERRIDE_DEBS)
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        fail("usage: prepare_libc6_validator.py <source-validator> <dest-validator> <override-deb-dir>")
    source = Path(argv[1]).resolve()
    dest = Path(argv[2]).resolve()
    override_deb_dir = Path(argv[3]).resolve()

    copy_validator(source, dest)
    copy_override_debs(override_deb_dir, dest / "tests/libc6/override-debs")
    patch_dockerfile(dest / "tests/libc6/Dockerfile")
    patch_install_hook(dest / "tests/_shared/install_override_debs.sh")
    print(f"prepare_libc6_validator: wrote patched checkout to {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
