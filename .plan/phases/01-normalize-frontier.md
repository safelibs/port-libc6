# Normalize Frontier

**Phase Name**

Normalize and commit the prepared phase-05 safe workspace frontier

**Implement Phase ID**

`impl_05a_commit_prepared_safe_frontier`

**Preexisting Inputs**

- The prepared but currently untracked `safe/**` workspace exactly as it exists on disk when this workflow begins, including `safe/.gitignore`, `safe/Cargo.toml`, `safe/Cargo.lock`, `safe/README.md`, `safe/rust-toolchain.toml`, `safe/crates/**`, `safe/xtask/**`, `safe/generated/**`, `safe/tests/**`, `safe/debian/**`, `safe/upstream-compat/*.toml`, `safe/upstream-tests/README.md`, `safe/scripts/build-debs.sh`, and `safe/scripts/install-safe-repo.sh`.
- Transient cache artifacts currently present under `safe/tests/**/__pycache__/` and `safe/tests/**/*.pyc`. Treat these as deletable non-authoritative byproducts, not landed source files.
- Read-only tracked authorities outside `safe/**`: `original/**`, `original/INSTALL`, `original/debian/rules`, `original/debian/rules.d/build.mk`, `dependents.json`, `relevant_cves.json`, and `test-original.sh`.
- If it already exists, the local derived verifier input `safe/work/original-build/**`. If absent, recreate only that ignored derived tree before running legacy package, install, and link checks. Do not list it as a required workflow input because it is generated or validated by this phase.

**New Outputs**

- The prepared `safe/**` frontier committed to git as the canonical starting point for all later implementor and checker diffs.
- A committed `safe/generated/baseline/committed-safe-frontier.txt` manifest containing a sorted newline-delimited list of every intentionally tracked file under `safe/**` after cache cleanup.
- A committed `safe/scripts/stage-original-build.sh` helper with the fixed interface `./scripts/stage-original-build.sh --source ../original --build work/original-build`.
- A landed baseline where the prepared root metadata, placeholder crate roots, authoritative `safe/generated/**` files, authoritative `safe/upstream-compat/*.toml` ledgers, `safe/tests/**`, `safe/debian/**`, `safe/upstream-tests/README.md`, existing `safe/scripts/{build-debs.sh,install-safe-repo.sh}`, and new `safe/scripts/stage-original-build.sh` are tracked in git.
- Passing current phase-04/05 verifier commands, phase-05 safety audit, all-DSO ABI shell verification, and package-install smoke checks against the landed baseline.
- A persistent but uncommitted and ignored `safe/work/original-build/**` tree when this phase had to recreate it locally, while `safe/target/**` and `safe/upstream-tests/build/**` remain ignored derived roots.
- Explicitly ignored Python cache patterns for `safe/**/__pycache__/` and `safe/**/*.pyc`, with preexisting cache files removed from the landed committed frontier.
- No phase-06 subsystem cutover. The landed manifests and ledgers still describe the pre-cutover phase-05 hybrid state.

**File Changes**

- `safe/**` only where needed to land the prepared frontier cleanly and reproducibly:
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

**Implementation Details**

- Treat the existing on-disk `safe/**` tree as input. Do not recollect, regenerate, or redesign the prepared ABI baselines, version scripts, package manifests, test catalog, test-port plan, safety ledgers, or copied test tree.
- Delete transient Python cache artifacts under `safe/**/__pycache__/` and `safe/**/*.pyc` before generating the frontier manifest. These files are not part of the authoritative landing surface.
- Create and commit `safe/scripts/stage-original-build.sh` from existing upstream-build staging logic. Use it before any landing verifier command that depends on `safe/work/original-build/**`.
- `safe/scripts/stage-original-build.sh` must be idempotent and must not invent a second interface. It takes exactly `--source ../original --build work/original-build`.
- The staging helper must validate a preexisting tree by checking at least `testroot.pristine/install.stamp`, `elf/ld-linux-x86-64.so.2`, `testrun.sh`, `iconvdata`, and `localedata`, then return success without rebuilding when those invariants hold.
- If validation fails or `safe/work/original-build/**` is absent, the helper must delete and recreate only `safe/work/original-build/**`, write `configparms` for the safe baseline install layout (`bindir=/usr/bin`, `rootsbindir=/usr/sbin`, `sbindir=/usr/sbin`, `libdir=/usr/lib64`, `slibdir=/usr/lib64`, `rtlddir=/usr/lib64`, `libexecdir=/usr/libexec`, `includedir=/usr/include`, `complocaledir=/usr/lib/locale`, `localedir=/usr/share/locale`, `i18ndir=/usr/share/i18n`, and `vardbdir=/var/db`), run out-of-tree `../original/configure` with at least `--prefix=/usr --disable-werror --disable-crypt --without-selinux --enable-bind-now --enable-fortify-source --enable-stack-protector=strong --with-timeoutfactor=25`, then run `make -j$(nproc)` and `make testroot.pristine/install.stamp`.
- Do not commit `safe/work/**`, call `ingest-baseline`, or overwrite any landed `safe/generated/**` authority while staging the upstream build tree.
- Generate `safe/generated/baseline/committed-safe-frontier.txt` from the normalized landed tree as a sorted list of every intentionally tracked file under `safe/**`, excluding only `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and the deleted Python cache artifacts.
- Do not satisfy this phase by hiding prepared content behind ignore rules. The committed frontier must include the prepared root metadata, placeholder `safe/crates/{aux-dsos,compat-asm}/**` trees, `safe/xtask/**`, every authoritative `safe/generated/**` file, `safe/tests/**`, `safe/debian/**`, `safe/upstream-compat/*.toml`, `safe/upstream-tests/README.md`, and `safe/scripts/**`.
- The only ignored paths allowed under `safe/**` after landing are `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, `safe/**/__pycache__/`, and `safe/**/*.pyc`.
- If a baseline build or package smoke exposes a concrete defect in the prepared frontier, limit fixes to normalization or reproducibility issues needed to land the phase-05 state cleanly. Do not mix phase-06 subsystem work into this phase.
- Keep `.github/**` out of scope unless a later verifier proves it is required for the port workflow.

**Verification Phases**

- `check_05a_frontier_tracking`
  - Type: `check`
  - Fixed `bounce_target`: `impl_05a_commit_prepared_safe_frontier`
  - Purpose: Prove the full prepared authoritative `safe/**` frontier is now the committed linear diff base by rerunning the full current phase-04/05 verifier surface, current phase-05 safety audit, and current package-install smoke buckets against that landed baseline. Also prove, through a committed frontier manifest, that every intended landed file is tracked and that only explicitly allowed derived or cache paths remain ignored and untracked.
  - Commands:
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

**Success Criteria**

- `check_05a_frontier_tracking` passes.
- `safe/generated/baseline/committed-safe-frontier.txt` exactly matches the full landed non-derived, non-cache `safe/**` filesystem frontier.
- Every path listed in that manifest is tracked in `HEAD`.
- The explicit `check_04_*` and `check_05_*` commands, the current phase-05 `audit-safety` mode, all-DSO ABI shell verification, and package-install smoke buckets pass.
- `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and only the explicit Python cache patterns remain ignored and untracked.
- The landed baseline is suitable as the single bounce target for all later phases.

**Git Commit Requirement**

The implementer must commit all phase-owned work to git before yielding.
