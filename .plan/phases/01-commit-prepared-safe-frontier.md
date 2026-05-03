# Phase Name
Commit Prepared Safe Frontier

## Implement Phase ID
`impl_05a_commit_prepared_safe_frontier`

## Preexisting Inputs
- The prepared but currently untracked `safe/**` workspace exactly as it exists on disk when this workflow begins, especially:
  - `safe/.gitignore`
  - `safe/Cargo.toml`
  - `safe/Cargo.lock`
  - `safe/README.md`
  - `safe/rust-toolchain.toml`
  - `safe/crates/**`
  - `safe/xtask/**`
  - `safe/generated/**`
  - `safe/generated/security/relevant-cves-index.json`
  - `safe/tests/**`
  - `safe/debian/**`
  - `safe/upstream-compat/*.toml`
  - `safe/upstream-tests/README.md`
  - `safe/scripts/build-debs.sh`
  - `safe/scripts/install-safe-repo.sh`
- Transient cache artifacts currently present under `safe/tests/**/__pycache__/` and `safe/tests/**/*.pyc`. Treat them as deletable non-authoritative byproducts rather than landed source files.
- Read-only tracked authorities that already exist outside `safe/**`:
  - `original/**`
  - `original/INSTALL`
  - `original/debian/rules`
  - `original/debian/rules.d/build.mk`
  - `dependents.json`
  - `relevant_cves.json`
  - `test-original.sh`
- If it already exists, the local derived verifier input `safe/work/original-build/**`; if it does not exist, recreate only that derived tree before running the legacy package, install, and link checks, and do not commit it.

## New Outputs
- The prepared `safe/**` frontier committed to git as the canonical starting point for all later implementor and checker diffs.
- A committed `safe/generated/baseline/committed-safe-frontier.txt` manifest that enumerates every intentionally tracked file under `safe/**` after cache cleanup.
- A committed `safe/scripts/stage-original-build.sh` helper with the fixed interface `./scripts/stage-original-build.sh --source ../original --build work/original-build`.
- A persistent but uncommitted and ignored `safe/work/original-build/**` tree when the landing phase had to recreate that derived verifier prerequisite locally.
- Explicit ignore coverage for `safe/**/__pycache__/` and `safe/**/*.pyc`, with any preexisting cache files removed from the landed committed frontier.
- No phase-06 subsystem cutover yet; manifests and ledgers still describe the pre-cutover phase-05 hybrid state.

## File Changes
- `safe/.gitignore`
- `safe/Cargo.toml`
- `safe/Cargo.lock`
- `safe/README.md`
- `safe/rust-toolchain.toml`
- `safe/crates/**`
- `safe/xtask/**`
- `safe/generated/**`
- `safe/tests/**`
- `safe/debian/**`
- `safe/upstream-compat/*.toml`
- `safe/upstream-tests/README.md`
- `safe/scripts/**`
- No root-level file changes are authorized in this phase.

## Implementation Details
- Treat the existing on-disk `safe/**` tree as input, not as something to recollect, regenerate, or redesign.
- Add and commit the prepared Rust workspace, generated baselines, copied tests, packaging files, existing scripts, root metadata, and ledgers in place so later phases have a reviewable git baseline.
- Before generating the landed frontier manifest, delete any transient Python cache artifacts under `safe/**/__pycache__/` and `safe/**/*.pyc`. Do not commit them.
- Create and commit `safe/scripts/stage-original-build.sh` from the existing upstream-build staging logic and use that helper before the landing verifier runs any command that depends on `safe/work/original-build/**`.
- `safe/scripts/stage-original-build.sh` must be idempotent and must keep the fixed interface `./scripts/stage-original-build.sh --source ../original --build work/original-build`.
- When validating an existing `safe/work/original-build/**` tree, check at least `testroot.pristine/install.stamp`, `elf/ld-linux-x86-64.so.2`, `testrun.sh`, `iconvdata`, and `localedata`, then return success without rebuilding if those invariants hold.
- If validation fails or the tree is absent, delete and recreate only `safe/work/original-build/**`, write `configparms` for the safe baseline install layout (`bindir=/usr/bin`, `rootsbindir=/usr/sbin`, `sbindir=/usr/sbin`, `libdir=/usr/lib64`, `slibdir=/usr/lib64`, `rtlddir=/usr/lib64`, `libexecdir=/usr/libexec`, `includedir=/usr/include`, `complocaledir=/usr/lib/locale`, `localedir=/usr/share/locale`, `i18ndir=/usr/share/i18n`, and `vardbdir=/var/db`), run out-of-tree `../original/configure` with at least `--prefix=/usr --disable-werror --disable-crypt --without-selinux --enable-bind-now --enable-fortify-source --enable-stack-protector=strong --with-timeoutfactor=25`, then run `make -j$(nproc)` followed by `make testroot.pristine/install.stamp`.
- Do not call `ingest-baseline`, do not rewrite committed `safe/generated/**` authorities, and do not commit `safe/work/**`.
- Generate `safe/generated/baseline/committed-safe-frontier.txt` from the normalized landed tree as a sorted newline-delimited list of every intentionally tracked file under `safe/**`, excluding only `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and the deleted Python cache artifacts.
- Do not satisfy this phase by hiding prepared content behind new ignore rules. The committed frontier must include the prepared root metadata files, placeholder crate roots, `safe/xtask/**`, every authoritative `safe/generated/**` file, `safe/tests/**`, `safe/debian/**`, `safe/upstream-compat/*.toml`, `safe/upstream-tests/README.md`, and `safe/scripts/**`.
- The only allowed ignored paths under `safe/**` after landing are `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and the exact Python cache patterns `safe/**/__pycache__/` and `safe/**/*.pyc`.
- If a baseline build or package smoke exposes a concrete defect in the prepared frontier, limit fixes to normalization or reproducibility issues needed to land the phase-05 state cleanly. Do not mix phase-06 subsystem work into this phase.
- Keep `.github/**` out of scope unless a later verifier proves it is required for the port workflow.
- Preserve the consume-existing-artifacts contract: `original/**` remains read-only, prepared ABI baselines and test authorities stay authoritative, and later phases must consume the committed landing point rather than regenerate it.

## Verification Phases
### `check_05a_frontier_tracking`
- `phase_id`: `check_05a_frontier_tracking`
- `type`: `check`
- `bounce_target`: `impl_05a_commit_prepared_safe_frontier`
- `purpose`: Prove the full prepared authoritative `safe/**` frontier is now the committed linear diff base by rerunning the full current phase-04 and phase-05 verifier surface, the current phase-05 safety audit, and the current package-install smoke buckets against that landed baseline while also proving, via a committed frontier manifest, that every intended landed file is tracked in git and that only the explicitly allowed derived or cache paths remain ignored and untracked.
- `commands`:
```bash
cd safe
test -x scripts/stage-original-build.sh
./scripts/stage-original-build.sh --source ../original --build work/original-build
test -f work/original-build/testroot.pristine/install.stamp
test -x work/original-build/elf/ld-linux-x86-64.so.2
test -f work/original-build/testrun.sh
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check_04_loader_tests
cargo run -p xtask -- check_04_loader_abi
cargo run -p xtask -- check_04_loader_tools
cargo run -p xtask -- check_05_core_runtime_tests
cargo run -p xtask -- check_05_core_runtime_abi
cargo run -p xtask -- check_05_runtime_tools
cargo run -p xtask -- check_05_base_dependent_smoke
cargo run -p xtask -- audit-safety \
  --verify-policy \
  --deny-unreviewed-unsafe \
  --deny-untracked-fallback-c \
  --require-cve-disposition
cargo run -p xtask -- check-abi --all-dsos
cargo run -p xtask -- package-deb --out work/debs
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
cd ..
git ls-files --error-unmatch safe/generated/baseline/committed-safe-frontier.txt >/dev/null
manifest_tmp="$(mktemp)"
frontier_tmp="$(mktemp)"
LC_ALL=C sort safe/generated/baseline/committed-safe-frontier.txt > "$manifest_tmp"
find safe \
  \( -path 'safe/work' -o -path 'safe/target' -o -path 'safe/upstream-tests/build' -o -type d -name '__pycache__' \) -prune \
  -o -type f ! -name '*.pyc' -print | LC_ALL=C sort > "$frontier_tmp"
diff -u "$manifest_tmp" "$frontier_tmp"
while IFS= read -r path; do
  git ls-files --error-unmatch "$path" >/dev/null
done < "$manifest_tmp"
test -z "$(git ls-files -- safe/work safe/target safe/upstream-tests/build)"
git check-ignore -q safe/work/original-build
git check-ignore -q safe/target
git check-ignore -q safe/upstream-tests/build
git check-ignore -q safe/tests/support/__pycache__/probe.pyc
git check-ignore -q safe/tests/scripts/probe.pyc
test -z "$(git ls-files --others --exclude-standard -- safe)"
unexpected_ignored="$(git ls-files --others -i --exclude-standard --directory -- safe | awk '
  $0 ~ /^safe\/(work\/|target\/|upstream-tests\/build\/)$/ {next}
  $0 ~ /^safe\/.*\/__pycache__\/$/ {next}
  $0 ~ /^safe\/.*\.pyc$/ {next}
  {print}
')"
test -z "$unexpected_ignored"
```

## Success Criteria
- `check_05a_frontier_tracking` passes.
- `safe/generated/baseline/committed-safe-frontier.txt` exactly matches the normalized landed non-derived, non-cache `safe/**` frontier.
- Every path listed in `safe/generated/baseline/committed-safe-frontier.txt` is tracked in `HEAD`.
- The explicit `check_04_*` and `check_05_*` commands, the current phase-05 `audit-safety` mode, `check-abi --all-dsos`, and the package-install smoke buckets all pass.
- `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and only the explicit Python cache patterns remain ignored and untracked.
- The landed baseline is suitable as the single bounce target for all later phases.

## Git Commit Requirement
Commit the landed frontier to git before yielding. The commit must include the phase-scoped tracked `safe/**` baseline, `safe/generated/baseline/committed-safe-frontier.txt`, and `safe/scripts/stage-original-build.sh`.
