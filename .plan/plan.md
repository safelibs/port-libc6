# libc6 Safe Port Plan

## Context

Current workspace state:

- The full upstream GNU C Library 2.39 source tree under `original/`, with the native GNU Make build system rooted at `original/Makefile`, `original/Makeconfig`, and `original/Makerules`, and the upstream version declared in `original/version.h`.
- A partially bootstrapped Rust workspace under `safe/` with seven current Cargo workspace members (`xtask`, `libc-support-tools`, `core-runtime`, `ldso`, `libc6`, `libpthread`, `libthread-db`), placeholder crate roots for future `compat-asm` and `aux-dsos` work under `safe/crates/`, authoritative status ledgers under `safe/upstream-compat/*.toml`, and Debian packaging scaffolding for the required Ubuntu 24.04 package set.
- Prepared on-disk ABI baselines for 20 DSOs under `safe/generated/baseline/abi/*.json`, 20 generated version scripts under `safe/generated/version-scripts/*.map`, a required-package install manifest with 1,692 total entries for the seven-package shipped surface on amd64 (1,691 `shipped` entries plus `/usr/lib/pt_chown` marked `omitted_on_amd64`), a test-install manifest with 1,769 staged entries (that shipped set plus 77 `testroot_only` harness assets), a 5,584-entry upstream test catalog, a matching 5,584-entry ownership plan, a 257-asset shared support subtree, and a 40-entry non-memory CVE inventory.
- An on-disk test tree under `safe/tests/**` with 1,293 entries already marked `ported` and 4,291 entries still `planned` in `safe/tests/manifest.toml`.
- A fixed shared harness topology under `safe/tests/**` that later phases must update in place rather than rediscover:
  - shared harness inputs already materialized on disk outside per-phase executable ownership include `safe/tests/support/**` as assigned by `safe/generated/baseline/test-port-plan.json` `support_subtree`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets under `safe/tests/scripts/**`
  - executable or generated roots already materialized from phases 02-05 are `safe/tests/core/**`, `safe/tests/csu/**`, `safe/tests/elf/**`, `safe/tests/libio/**`, `safe/tests/malloc/**`, `safe/tests/misc/**`, `safe/tests/nptl/**`, `safe/tests/nptl_db/**`, `safe/tests/resolv/**`, `safe/tests/setjmp/**`, `safe/tests/signal/**`, `safe/tests/stdlib/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`
  - `safe/generated/baseline/test-port-plan.json` already fixes the remaining future roots that phases 06-09 must materialize exactly as assigned: phase 06 adds `assert`, `ctype`, `dirent`, `io`, `stdio-common`, `string`, `termios`, `time`, and `timezone`; phase 07 adds `hesiod`, `inet`, `nis`, `nss`, `socket`, and zero-entry `nscd` materialized by `safe/tests/nscd/.gitkeep`; phase 08 adds `conform`, `iconv`, `iconvdata`, `locale`, `localedata`, `posix`, and zero-entry `po` materialized by `safe/tests/po/.gitkeep`; phase 09 adds `argp`, `catgets`, `debug`, `dlfcn`, `gmon`, `gnulib`, `intl`, `login`, `math`, `mathvec`, zero-entry `manual` materialized by `safe/tests/manual/.gitkeep`, `resource`, `rt`, `sunrpc`, `sysvipc`, `wcsmbs`, and `wctype`

Current limitations:

- `safe/xtask/src/commands/build.rs` currently emits zero-return “hybrid ABI shells” for symbol-surface verification only; it does not build runnable Rust DSOs yet.
- `safe/generated/baseline/package-files/libc6.json` still stages `libc.so.6`, `ld-linux-x86-64.so.2`, `libpthread.so.0`, `libresolv.so.2`, `libm.so.6`, and the other shipped DSOs from logical `build/testroot.pristine/...` source paths with `source_origin = "build_testroot"`, which `xtask` resolves against the staged upstream build tree under `safe/work/original-build/**` when that tree is present. The current `.deb` payload therefore still comes from baseline glibc.
- `safe/generated/baseline/package-files/libc6-dev.json` still stages the code-bearing development-link payload from `build/testroot.pristine`, including `/usr/lib64/{Mcrt1.o,Scrt1.o,crt1.o,crti.o,crtn.o,gcrt1.o,grcrt1.o,rcrt1.o,libBrokenLocale.a,libanl.a,libc.a,libc_nonshared.a,libdl.a,libg.a,libm-2.39.a,libm.a,libmcheck.a,libmvec.a,libpthread.a,libresolv.a,librt.a,libutil.a}`, the public `.so` link names and `audit/sotruss-lib.so` under `/usr/lib64/**`, and Debian’s empty `libpthread_nonshared.a`. Those startup objects, static archives, and development-link DSOs are a separate final cutover obligation: the workflow cannot treat them as implicitly solved by moving the runtime DSOs off `build_testroot`.
- Only a few public entrypoints have Rust frontends today: `ld.so`, `ldd`, `ldconfig`, and `pldd`, all tracked through `safe/crates/libc-support-tools/src/fallback.rs`.
- The phase-refresh helpers are hard-coded to the phase-05 frontier in `safe/xtask/src/common.rs:12-23` and `safe/xtask/src/commands/build.rs:23-31,410-980`; later phases must make this machinery data-driven so phase-specific verifiers remain stable, later owned test trees can be materialized, and `source_path == null` placeholder assets can be generated outside phase 05.
- `safe/crates/libc-support-tools/src/fallback.rs:48-237` still declares fallback wrappers for every remaining phase-owned shipped tool (`getent`, `nscd`, locale tools, and dev/time tools), and `safe/xtask/src/commands/test_package_install.rs:18-23` still only recognizes the phase-05 smoke buckets (`basic-required-packages`, `loader-tools`, `runtime-tools`).
- `safe/xtask/src/commands/link_compat_smoke.rs:62-137` is still source-based: it recompiles fresh C snippets directly against `work/install-root`, so it does not yet prove that object files previously compiled against the original libc6, including original test objects from the staged upstream build, can be relinked and run against the safe install root, and there is not yet any committed relink-corpus artifact under `safe/generated/**` that later phases could consume instead of rediscovering cases ad hoc.
- `safe/work/original-build/**` is not checked in, the current workspace snapshot does not contain it, and the current workflow has no dedicated command that materializes that staged upstream build tree without also rerunning broader baseline-ingestion logic. The existing phase-04/05 package, install, upstream-test, and link-smoke verifiers all consume that tree, so the landing phase must commit a concrete helper that can recreate or validate only this derived tree without touching committed authorities, and phase 06 must expose the same logic as a dedicated reusable `xtask` command that later checkers invoke explicitly.

Repository state:

- Relative to `HEAD`, the prepared `safe/**` workspace is not yet committed in git; `git diff f87362aca3d918f740063d16cd1dd4bd250fd7de..HEAD` is empty and `git status --short --untracked-files=all` reports `safe/` as untracked.
- The current snapshot has no `safe/work/original-build/**` tree.
- `safe/scripts/` currently contains only `build-debs.sh` and `install-safe-repo.sh`; `safe/scripts/stage-original-build.sh` does not exist yet.
- The workflow cannot assume that phases 02-05 already exist as reviewable commits or that the staged upstream build prerequisite is present.
- This plan starts with `impl_05a_commit_prepared_safe_frontier`, which commits the prepared `safe/**` tree, creates and commits `safe/scripts/stage-original-build.sh`, and uses that helper when needed to recreate only the derived `safe/work/original-build/**` tree locally so the existing phase-04/05 verifiers can run.
- Every later reference to a committed `safe/**` artifact means the post-`impl_05a_commit_prepared_safe_frontier` state.

Transient caches:

- The prepared tree currently contains transient Python bytecode caches under `safe/tests/**/__pycache__/` and `safe/tests/**/*.pyc`. Phase `05a` must delete them before landing the baseline, then ignore only those exact cache patterns.

Plan goals:

1. Preserve the prepared artifact contracts instead of regenerating them.
2. Convert the current “baseline payload plus Rust-side scaffolding” state into a true incremental Rust implementation that can eventually replace every shipped libc6 runtime surface.

Incremental architecture:

- Keep the prepared ABI JSON files and version scripts as the symbol/SONAME oracles; `impl_05a_commit_prepared_safe_frontier` commits them before later phases update them in place.
- Replace the current stub-only hybrid shells with generated x86_64 export veneers that forward every still-unported symbol to a privately staged baseline backend DSO using exact symbol-version lookups; stage every such non-public DSO under a dedicated backend namespace such as `/usr/libexec/safelibs/backends/**`, record it in package manifests and `package-scope.toml` as `asset_kind = "private_baseline_backend_dso"`, and record it in `fallback-c-inventory.json` as `classification = "private_baseline_backend_dso"`.
- Let Rust `extern "C"` exports override those veneers automatically as each subsystem is ported.
- Keep the private backend copies only through phases 06-09, then make phase 10 fail the build if any shipped symbol would still need backend forwarding.

This architecture preserves runtime and link compatibility while porting `libc.so.6` and related DSOs across linear phases.

## Generated Workflow Contract

The workflow must follow these fixed rules:

- Execution is strictly linear. Do not use `parallel_groups`.
- The workflow YAML must be fully self-contained and inline-only. Do not use top-level `include`, and do not use phase-level `prompt_file`, `workflow_file`, `workflow_dir`, `checks`, or any other YAML-source indirection.
- Use only fixed `bounce_target` fields. Do not use agent-guided `bounce_targets` lists.
- Every verifier must be an explicit top-level `check` phase.
- Every verifier must remain grouped under the implement phase it verifies and must bounce only to that implement phase.
- If a verifier needs to run tests, lint, build, package, Docker, or any other command, those commands must be written directly into the checker instructions. Do not model them as separate non-agentic phases.
- Any generated checker that invokes `cargo run -p xtask -- link-compat-smoke ...` must describe the upgraded semantics explicitly: the command must relink object files built against the original libc6 from `safe/work/original-build/**` or from an original-sysroot fixture build, and then run the resulting binaries. It must not satisfy link compatibility by compiling fresh sources against the safe install root.
- From phase 06 onward, any generated checker that invokes `cargo run -p xtask -- link-compat-smoke ...` must consume a committed relink oracle at `safe/generated/baseline/link-compat-corpus.json`. That file must enumerate every relink case by stable case ID, owner phase, coverage class, object source kind, fixture source when applicable, preserved-object path, required startfiles or archives, exercised public surfaces, and run mode. Later phases may extend that one file in place, but no checker may rediscover relink coverage procedurally by scanning `safe/work/original-build/**` alone.
- From phase 06 onward, any generated checker whose commands consume `safe/work/original-build/**` must explicitly run `cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build` before the first consuming command. This applies to checker sequences that invoke `check_04_loader_tests`, `check_05_core_runtime_tests`, `check_05_base_dependent_smoke`, `check-owned-tests`, `run-original-tests`, `link-compat-smoke`, `package-deb`, or `test-package-install`. The helper must be idempotent: validate and reuse a correct tree, recreate only when absent or corrupt, and never rewrite committed `safe/generated/**` authorities.
- Any generated checker that validates `libc6-dev` must distinguish code-bearing link assets from copied headers or data. By the end of phase 10, every shipped `libc6-dev` entry under `/usr/lib*/**` that participates in startup, static linkage, profiling, audit, or DSO link-time resolution (`*.o`, `*.a`, public `*.so`, `/usr/lib*/audit/*.so`, and Debian’s `libpthread_nonshared.a`) must have safe-owned provenance recorded as one of `rust_build`, `compat_asm`, `compat_archive`, `compat_linker_script`, `synthetic_empty_archive`, or a symlink to a safe-built DSO; none of those final code-bearing entries may keep `source_origin = "build_testroot"` or a `build/testroot.pristine/usr/lib*/...` source path. If a final `libc6-dev` entry remains non-Rust, `safe/upstream-compat/package-scope.toml` and `safe/upstream-compat/safety-policy.toml` must say why it is unavoidable and why the remaining unsafe boundary is acceptable.
- Any generated checker that validates backend removal must treat private baseline backend DSOs as a distinct tracked class. From phase 06 onward, every shipped backend DSO used only for symbol forwarding must live under `/usr/libexec/safelibs/backends/**` or an equivalently dedicated non-public prefix, use `asset_kind = "private_baseline_backend_dso"` in `safe/generated/baseline/package-files/*.json`, `safe/generated/install-manifests/*.json`, and `safe/upstream-compat/package-scope.toml`, and use `classification = "private_baseline_backend_dso"` in `safe/generated/baseline/fallback-c-inventory.json`. Phase 10 must have an explicit checker path that fails if any shipped entry with that classification or path prefix remains in those files or in the installed package filesystem.
- Any generated checker that validates phase-owned upstream tests must use an owner-phase-aware `xtask` test checker that selects exact catalog rows from `safe/generated/baseline/test-port-plan.json` and `safe/tests/manifest.toml`. Do not approximate phase ownership with broad `run-original-tests --subdirs ...` filters, because logical subdirs such as `sysdeps`, `resolv`, `stdlib`, and `libio` are shared across phases.
- Every zero-entry destination listed in `safe/generated/baseline/test-port-plan.json` must be materialized and verified through a committed sentinel file named `<destination_root>/.gitkeep`. A bare empty directory is not a valid artifact because git will not track it. Generated implement prompts and checkers must name and validate those exact sentinel paths.
- Every implement prompt must explicitly tell the implementor to commit the phase’s work to git before yielding.
- Any generated git-tracking or cleanliness assertion must be path-scoped to that phase’s declared file set. Do not require whole-repository cleanliness, because unrelated paths outside the phase scope may already be dirty before the workflow starts. For `impl_05a_commit_prepared_safe_frontier`, the only allowed ignored paths under `safe/**` are `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and non-authoritative Python cache artifacts matching `safe/**/__pycache__/` or `safe/**/*.pyc`; every other materialized file under `safe/**` must be committed and tracked.
- Any phase that changes shipped package payload membership, shipped helper paths, or installed-package provenance must update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits. Generated package-install verifiers must consume those committed install manifests directly rather than synthesizing replacements.
- Existing prepared artifacts are authoritative inputs, but the workflow must first normalize the on-disk `safe/**` tree into a committed git baseline before later phases rely on it. `impl_05a_commit_prepared_safe_frontier` must land the prepared `safe/**` tree in place rather than regenerating it, and every later phase must consume that committed landing point:
  - `original/**`
  - `dependents.json`
  - `relevant_cves.json`
  - `safe/generated/security/relevant-cves-index.json`
  - `safe/generated/baseline/abi/*.json`
  - `safe/generated/version-scripts/*.map`
  - `safe/generated/baseline/package-files/*.json`
  - `safe/generated/install-manifests/*.json`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/tests/manifest.toml`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
  - `test-original.sh`
- Preserve the consume-existing-artifacts contract explicitly:
  - Do not mutate `original/**`; treat it as read-only source truth.
  - During `impl_05a_commit_prepared_safe_frontier`, do not replace the prepared ABI baselines or test catalog with newly collected data. After that landing phase commits them, treat those files as authoritative and only update them if a checker proves the existing generator is wrong.
  - During `impl_05a_commit_prepared_safe_frontier`, create and commit `safe/generated/baseline/committed-safe-frontier.txt` as a sorted newline-delimited manifest of every intentionally tracked file under `safe/**` after deleting the non-authoritative Python cache artifacts named above. Later phases treat that file as the exact mechanical record of what the landing phase committed; they do not regenerate it.
  - During `impl_05a_commit_prepared_safe_frontier`, rerun the prepared phase-04/05 verifier surface explicitly by invoking the existing `check_04_*` and `check_05_*` `xtask` commands, the current phase-05 `audit-safety` mode, and the current package-install smoke buckets before any later phase may rely on the landed baseline.
  - Do not re-infer package scope from `original/debian/**`; extend the phase-05a-committed packaging surface under `safe/debian/**` and `safe/generated/packaging/package-build-manifest.json`.
  - Do not replace `safe/generated/baseline/test-port-plan.json`; port tests into the destinations it already assigns.
  - Treat `safe/tests/support/**`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets that `impl_05a_commit_prepared_safe_frontier` first commits under `safe/tests/scripts/{check-c++-types.sh,check-execstack.awk,check-initfini.awk,check-installed-headers.sh,check-local-headers.sh,check-localplt.awk,check-obsolete-constructs.py,check-textrel.awk,check-wrapper-headers.py,check-wx-segment.py,lint-makefiles.sh}` as existing shared inputs. When multiple catalog IDs map to the same shared file, update `safe/tests/manifest.toml` ownership/status in place instead of creating duplicate copies.
  - Respect `safe/generated/baseline/test-port-plan.json` `zero_entry_subdirs` exactly as checked in: phases 07-09 materialize `safe/tests/nscd`, `safe/tests/po`, and `safe/tests/manual` by committing `safe/tests/nscd/.gitkeep`, `safe/tests/po/.gitkeep`, and `safe/tests/manual/.gitkeep` respectively, and those owned destinations have no `run-original-tests` command coverage.
  - Respect the normalized sysdeps destinations created by `safe/xtask/src/commands/build.rs`: catalog subdir `sysdeps` entries can materialize into `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`, and the workflow must name those concrete trees where phases touch them.
  - Preserve the exact workspace-test root topology from `safe/generated/baseline/test-port-plan.json`: later phases must materialize the missing top-level roots named there and must not collapse them into broader umbrella trees, invent alternate root names, or hide them behind `run-original-tests --subdirs` approximations.
  - Do not create ad hoc safety ledgers; update `safe/generated/baseline/fallback-c-inventory.json`, `safe/upstream-compat/cve-status.toml`, `safe/upstream-compat/package-scope.toml`, and `safe/upstream-compat/safety-policy.toml` in place.
- `safe/work/original-build/**` is a derived workspace artifact rather than a checked-in authority. During `impl_05a_commit_prepared_safe_frontier`, create and commit `safe/scripts/stage-original-build.sh` and use it whenever `safe/work/original-build/testroot.pristine/install.stamp` or companion harness outputs are missing. Its interface is fixed as `./scripts/stage-original-build.sh --source ../original --build work/original-build`; it must validate an existing tree in place, otherwise recreate only `safe/work/original-build/**` via an out-of-tree upstream build with the safe baseline install layout (`bindir=/usr/bin`, `rootsbindir=/usr/sbin`, `sbindir=/usr/sbin`, `libdir=/usr/lib64`, `slibdir=/usr/lib64`, `rtlddir=/usr/lib64`, `libexecdir=/usr/libexec`, `includedir=/usr/include`, `complocaledir=/usr/lib/locale`, `localedir=/usr/share/locale`, `i18ndir=/usr/share/i18n`, and `vardbdir=/var/db`), followed by `make -j$(nproc)` and `make testroot.pristine/install.stamp`. It must never call `ingest-baseline` or rewrite committed `safe/generated/**` authorities.
- Phase 06 must codify that exact recreation and validation logic as `cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build`. Later phases must call that command explicitly before any checker step that consumes the staged tree and must then reuse the resulting workspace state in place instead of rebuilding it unless the helper proves the tree is corrupt.
- The workflow starts by landing the prepared but currently untracked phase-05 frontier through `impl_05a_commit_prepared_safe_frontier`. Only after that landing commit may new feature work begin at `impl_06_io_stdio_string_path`.

## Implementation Phases

Prepared but uncommitted frontier before this workflow starts:

- `impl_02_hybrid_abi_shell`
- `impl_03_packaging_and_harness`
- `impl_04_loader_startup_secure_exec`
- `impl_05_core_runtime_threads_entropy`

Workflow order: `impl_05a_commit_prepared_safe_frontier`, then phases `impl_06_*` through `impl_10_*`.

### 1. Normalize and commit the prepared phase-05 safe workspace frontier

**Implement Phase ID**

`impl_05a_commit_prepared_safe_frontier`

**Verification Phases**

- `check_05a_frontier_tracking`
  - Type: `check`
  - Fixed `bounce_target`: `impl_05a_commit_prepared_safe_frontier`
  - Purpose: Prove the full prepared authoritative `safe/**` frontier is now the committed linear diff base by rerunning the full current phase-04/05 verifier surface, the current phase-05 safety audit, and the current package-install smoke buckets against that landed baseline while also proving, via a committed frontier manifest, that every intended landed file is tracked in git and that only the explicitly allowed derived or cache paths remain ignored and untracked.
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

**Preexisting Inputs**

- The prepared but currently untracked `safe/**` workspace exactly as it exists on disk when this workflow begins, especially:
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
  - `safe/scripts/build-debs.sh`
  - `safe/scripts/install-safe-repo.sh`
  - transient cache artifacts currently present under `safe/tests/**/__pycache__/` and `safe/tests/**/*.pyc`, which this phase must treat as deletable non-authoritative byproducts rather than landed source files
- Read-only tracked authorities that already exist outside `safe/**`:
  - `original/**`
  - `original/INSTALL`
  - `original/debian/rules`
  - `original/debian/rules.d/build.mk`
  - `dependents.json`
  - `relevant_cves.json`
  - `test-original.sh`
- If it already exists, the local derived verifier input `safe/work/original-build/**`; if it does not exist, this phase must recreate only that derived tree before running the legacy package/install/link checks and must not commit it.

**New Outputs**

- The prepared `safe/**` frontier committed to git as the canonical starting point for all later implementor/checker diffs.
- A committed `safe/generated/baseline/committed-safe-frontier.txt` manifest that enumerates every intentionally tracked file under `safe/**` after cache cleanup, and a landed baseline where that full manifest, the prepared root metadata (`safe/.gitignore`, `safe/Cargo*.toml`, `safe/README.md`, `safe/rust-toolchain.toml`), placeholder crate roots, authoritative `safe/generated/**` files, authoritative `safe/upstream-compat/*.toml` ledgers, `safe/tests/**`, `safe/debian/**`, `safe/upstream-tests/README.md`, the preexisting `safe/scripts/{build-debs.sh,install-safe-repo.sh}`, and the new `safe/scripts/stage-original-build.sh` are all tracked in git, the current phase-04/05 verifier commands, the current phase-05 safety audit, the all-DSO ABI shell verification, and the package-install smoke checks all pass without leaving unexpected `safe/**` drift, and no path outside the explicit ignore allowlist remains ignored or untracked.
- A committed `safe/scripts/stage-original-build.sh` helper with the fixed interface `./scripts/stage-original-build.sh --source ../original --build work/original-build`. It must validate the staged upstream build tree in place when present, otherwise recreate only `safe/work/original-build/**` using the exact safe-baseline install layout and then stop without mutating committed `safe/generated/**` authorities.
- A persistent but uncommitted and ignored `safe/work/original-build/**` workspace tree when the landing phase had to recreate that derived verifier prerequisite locally, while `safe/target/**` and `safe/upstream-tests/build/**` also remain derived ignored roots.
- Explicitly ignored Python cache patterns for `safe/**/__pycache__/` and `safe/**/*.pyc`, with any preexisting cache files removed from the landed committed frontier.
- No phase-06 subsystem cutover yet; the landed manifests and ledgers still describe the pre-cutover phase-05 hybrid state.

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

- Treat the existing on-disk `safe/**` tree as input, not as something to recollect, regenerate, or redesign.
- Add and commit the prepared Rust workspace, generated baselines, copied tests, packaging files, existing scripts, root metadata, and ledgers in place so later phases have a reviewable git baseline.
- Before generating the landed frontier manifest, delete any transient Python cache artifacts under `safe/**/__pycache__/` and `safe/**/*.pyc`. These files are not part of the authoritative landing surface and must not be committed.
- Create and commit `safe/scripts/stage-original-build.sh` in this phase from the existing upstream-build staging logic and use that exact helper before the landing verifier runs any command that depends on `safe/work/original-build/**`.
- `safe/scripts/stage-original-build.sh` must be idempotent and must not invent its own interface. It takes exactly `--source ../original --build work/original-build`, validates a preexisting tree by checking at least `testroot.pristine/install.stamp`, `elf/ld-linux-x86-64.so.2`, `testrun.sh`, `iconvdata`, and `localedata`, and returns success without rebuilding when those invariants hold.
- If validation fails or `safe/work/original-build/**` is absent, `safe/scripts/stage-original-build.sh` must delete and recreate only `safe/work/original-build/**`, write `configparms` for the safe baseline install layout (`bindir=/usr/bin`, `rootsbindir=/usr/sbin`, `sbindir=/usr/sbin`, `libdir=/usr/lib64`, `slibdir=/usr/lib64`, `rtlddir=/usr/lib64`, `libexecdir=/usr/libexec`, `includedir=/usr/include`, `complocaledir=/usr/lib/locale`, `localedir=/usr/share/locale`, `i18ndir=/usr/share/i18n`, and `vardbdir=/var/db`), run an out-of-tree `../original/configure` with at least `--prefix=/usr --disable-werror --disable-crypt --without-selinux --enable-bind-now --enable-fortify-source --enable-stack-protector=strong --with-timeoutfactor=25`, then run `make -j$(nproc)` followed by `make testroot.pristine/install.stamp`.
- Do not commit `safe/work/**`, do not call `ingest-baseline`, and do not overwrite any landed `safe/generated/**` authority while staging the upstream build tree.
- Generate `safe/generated/baseline/committed-safe-frontier.txt` from the normalized landed tree as a sorted newline-delimited list of every intentionally tracked file under `safe/**`, excluding only `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and the deleted Python cache artifacts. Commit that manifest in the same phase-05a git commit as the rest of the landed frontier.
- Do not satisfy this landing phase by hiding prepared content behind ignore rules. The committed frontier must include the prepared root metadata files, the placeholder `safe/crates/{aux-dsos,compat-asm}/**` trees, `safe/xtask/**`, every authoritative `safe/generated/**` file, `safe/tests/**`, `safe/debian/**`, `safe/upstream-compat/*.toml`, `safe/upstream-tests/README.md`, and `safe/scripts/**`. The only ignored paths allowed under `safe/**` after landing are `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and the exact Python cache patterns `safe/**/__pycache__/` and `safe/**/*.pyc`.
- If a baseline build or package smoke exposes a concrete defect in the prepared frontier, limit fixes to normalization or reproducibility issues needed to land the phase-05 state cleanly. Do not mix phase-06 subsystem work into this phase.
- Keep `.github/**` out of scope unless a later verifier proves it is required for the port workflow.
- Commit the landed frontier before yielding; every later implementor and checker must diff against this commit rather than against the preexisting untracked workspace state.

**Verification**

Run `check_05a_frontier_tracking`. The phase is not complete until `safe/generated/baseline/committed-safe-frontier.txt` exactly matches the full landed non-derived, non-cache `safe/**` filesystem frontier, every path listed in that manifest is tracked in `HEAD`, the explicit `check_04_*` and `check_05_*` commands plus the current phase-05 `audit-safety` mode all pass, the package-install smoke buckets pass, `safe/work/**`, `safe/target/**`, `safe/upstream-tests/build/**`, and only the explicit Python cache patterns remain ignored and untracked, and the landed baseline is suitable as the single bounce target for all later phases.

### 2. I/O, stdio, string, path, time, and first real libc-family cutover

**Implement Phase ID**

`impl_06_io_stdio_string_path`

**Verification Phases**

- `check_06_phase_metadata_backcompat`
  - Type: `check`
  - Fixed `bounce_target`: `impl_06_io_stdio_string_path`
  - Purpose: Prove the phase-metadata refactor did not break the landed phase-04/05 verifier commands or rename their CLI entrypoints.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check_04_loader_tests
    cargo run -p xtask -- check_04_loader_abi
    cargo run -p xtask -- check_04_loader_tools
    cargo run -p xtask -- check_05_core_runtime_tests
    cargo run -p xtask -- check_05_core_runtime_abi
    cargo run -p xtask -- check_05_runtime_tools
    cargo run -p xtask -- check_05_base_dependent_smoke
    ```
- `check_06_io_stdio_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_06_io_stdio_string_path`
  - Purpose: Validate the phase-owned copied test tree and run the phase-owned upstream tests against the Rust-backed install root.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-owned-tests \
      --owner-phase impl_06_io_stdio_string_path \
      --root work/install-root \
      --build-root work/original-build \
      --privileged-container-tests
    ```
- `check_06_io_stdio_abi`
  - Type: `check`
  - Fixed `bounce_target`: `impl_06_io_stdio_string_path`
  - Purpose: Verify ABI, SONAME, versioned exports, original-object relink compatibility, and headers for the libc-family surfaces that phase 06 first moves off baseline payloads.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --dso libc --dso libpthread --dso libthread_db --dso libc_malloc_debug --dso libmemusage
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
    cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
    ```
- `check_06_io_stdio_packages`
  - Type: `check`
  - Fixed `bounce_target`: `impl_06_io_stdio_string_path`
  - Purpose: Ensure the `.deb` packages now stage Rust-built public libc-family DSOs, prove installed-payload provenance for the first libc-family cutover, and rerun the already-shipped loader/runtime tool behaviors after that cutover.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
    ```
- `check_06_io_stdio_safety`
  - Type: `check`
  - Fixed `bounce_target`: `impl_06_io_stdio_string_path`
  - Purpose: Enforce the stronger safety mode once libc-family exported symbols start running through Rust code or generated forwarding veneers.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --require-cve-disposition
    ```

**Preexisting Inputs**

- All outputs from `impl_05a_commit_prepared_safe_frontier`, which commit the prepared phase-02 through phase-05 workspace state into git, especially:
  - `safe/xtask/src/common.rs`
  - `safe/xtask/src/commands/build.rs`
  - `safe/generated/baseline/committed-safe-frontier.txt`
  - `safe/generated/baseline/abi/libc.json`
  - `safe/generated/baseline/abi/libpthread.json`
  - `safe/generated/baseline/abi/libthread_db.json`
  - `safe/generated/baseline/abi/libc_malloc_debug.json`
  - `safe/generated/baseline/abi/libmemusage.json`
  - `safe/generated/version-scripts/libc.map`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc6-dev.json`
  - `safe/generated/baseline/package-files/libc6-dbg.json`
  - `safe/tests/manifest.toml`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/*.toml`
  - `safe/scripts/stage-original-build.sh`

**New Outputs**

- A dedicated `stage-upstream-build` `xtask` path that mirrors the committed `safe/scripts/stage-original-build.sh` semantics and can create or validate the persistent staged upstream build tree under `safe/work/original-build/**`, including `testroot.pristine`; if phase `05a` already recreated that tree locally, phase 06 adopts it in place instead of rebuilding it.
- A committed relink oracle at `safe/generated/baseline/link-compat-corpus.json` that fixes the initial original-object relink matrix for later phases instead of letting them rediscover coverage ad hoc.
- A real Rust-backed build path for the libc-family DSOs, with generated forwarding veneers instead of zero-return stubs.
- Updated package manifests, install manifests, package-scope entries, and installed-package cutover smokes that point the shipped `libc6` payload and the `libc6-dev` public `.so` link names at Rust-built public DSOs plus private baseline backend copies, while explicitly inventorying any still-temporary `libc6-dev` startup/static/audit artifacts that remain on `build_testroot` until phase 10.
- Ported test sources for the 654 phase-owned entries covering `stdio-common`, `stdlib`, `libio`, `string`, `io`, `time`, `dirent`, `assert`, `ctype`, `termios`, `timezone`, the shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` rows, the generated per-subdir `check-installed-headers-*` placeholders that live under those owned roots, and the normalized `safe/tests/{sysdeps,sysdeps-x86_64,sysdeps-linux-x86_64}/**` destinations.
- Updated safety and CVE status ledgers for the runtime/path/time issues that stop being “baseline backend” exceptions once the public libc-family DSOs switch over.

**File Changes**

- Refactor phase-metadata machinery:
  - `safe/xtask/src/common.rs`
  - `safe/xtask/src/commands/build.rs`
  - `safe/xtask/src/commands/ingest_baseline.rs`
  - `safe/xtask/src/commands/stage_upstream_build.rs`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - `safe/xtask/src/commands/check_04_loader_tests.rs`
  - `safe/xtask/src/commands/check_04_loader_abi.rs`
  - `safe/xtask/src/commands/check_04_loader_tools.rs`
  - `safe/xtask/src/commands/check_05_core_runtime_tests.rs`
  - `safe/xtask/src/commands/check_05_core_runtime_abi.rs`
  - `safe/xtask/src/commands/check_05_runtime_tools.rs`
  - `safe/xtask/src/commands/check_05_base_dependent_smoke.rs`
  - `safe/xtask/src/commands/run_original_tests.rs`
  - `safe/xtask/src/main.rs`
  - `safe/xtask/src/commands/mod.rs`
- Build/install/package cutover:
  - `safe/xtask/src/commands/install_root.rs`
  - `safe/xtask/src/commands/package_deb.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/xtask/src/commands/check_headers.rs`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc6-dev.json`
  - `safe/generated/baseline/package-files/libc6-dbg.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/scripts/install-safe-repo.sh`
- Core implementation:
  - `safe/crates/libc6/src/lib.rs`
  - `safe/crates/libc6/src/sys/**`
  - new phase-06 modules under `safe/crates/libc6/src/` for stdio, string, io, time, and path-facing exports
  - `safe/crates/core-runtime/src/**`
  - `safe/crates/libpthread/src/**`
  - `safe/crates/libthread-db/src/**`
  - new generated or checked-in veneer sources under `safe/crates/compat-asm/x86_64/**`
- Test tree and phase checks:
  - `safe/tests/manifest.toml`
  - phase-owned files under `safe/tests/stdio-common/**`, `safe/tests/stdlib/**`, `safe/tests/libio/**`, `safe/tests/string/**`, `safe/tests/io/**`, `safe/tests/time/**`, `safe/tests/dirent/**`, `safe/tests/assert/**`, `safe/tests/ctype/**`, `safe/tests/termios/**`, `safe/tests/timezone/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`, including generated `check-installed-headers-*`, `test-as-const-*`, compare-output fixtures, and other `source_path == null` placeholders assigned to those roots
  - the shared phase-owned script rows at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`, whose manifest rows must be marked `ported` in place without duplicating the files
- Status and safety ledgers:
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
  - `safe/README.md`

**Implementation Details**

- Replace the current stub-only hybrid-shell generator in `safe/xtask/src/commands/build.rs` with a real incremental DSO builder.
  - Keep using the checked-in ABI JSON and generated version scripts as the authoritative export list.
  - Generate x86_64 assembly veneers for every exported symbol that is still missing from the Rust object set for that DSO.
  - Resolve forwarded symbols by exact version using `dlvsym` from a privately staged baseline backend DSO, not by ordinary `dlsym`.
  - Bind versioned public names with `.symver` so the public ABI stays byte-for-byte compatible with the existing version scripts.
  - Make the build fail if a Rust-provided symbol collides with a generated fallback veneer name, or if a baseline export cannot be resolved in the private backend copy.
- Stop using `safe/xtask/src/common.rs:12-27` and the phase-05-specific refresh logic in `safe/xtask/src/commands/build.rs:410-980` as a moving global frontier.
  - Replace single mutable frontier constants with explicit per-command expected owner IDs and a data-driven current frontier list.
  - Preserve the existing CLI command names `check_04_loader_tests`, `check_04_loader_abi`, `check_04_loader_tools`, `check_05_core_runtime_tests`, `check_05_core_runtime_abi`, `check_05_runtime_tools`, and `check_05_base_dependent_smoke`; phase 06 must not delete, rename, or silently repurpose them.
  - Rewrite those legacy command modules as thin wrappers over the new phase-aware helpers, each with its own fixed owner phase, DSO set, helper-path assertions, and install-root defaults rather than a shared global `PHASE_ID`.
  - `check_04_loader_tests.rs` and `check_05_core_runtime_tests.rs` must continue enforcing their current extra invariants after the refactor: the normalized `sysdeps*` destination expectations for phase 04 and the phase-05 stdlib allowlist for `tst-arc4random-*` plus `tst-getrandom`.
  - `safe/xtask/src/main.rs` and `safe/xtask/src/commands/mod.rs` must continue registering those legacy subcommands under the same names while also adding `stage-upstream-build` and `check-owned-tests`.
  - Stop hard-coding phase-05 prose into generated READMEs and manifest notes.
  - Preserve already-committed later-phase `safe/tests/manifest.toml` status changes in place instead of regenerating every post-phase-05 row back to `planned` from `COMPLETED_PHASES`.
- Factor the staged upstream-build preparation out of the broader baseline-ingestion path.
  - Introduce a dedicated `cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build` command that materializes `safe/work/original-build/**` without rewriting the checked-in generated baselines.
  - Do not invent a second build recipe in phase 06. `stage-upstream-build` must either call the committed `safe/scripts/stage-original-build.sh` helper or share the same validation list, `configparms` values, `configure` flags, and `make testroot.pristine/install.stamp` sequence that phase `05a` committed.
  - If phase `05a` already recreated `safe/work/original-build/**`, make the new command validate and adopt that tree in place; otherwise create it here exactly once. In either case, treat the resulting tree as persistent workspace state for phases 07-10, `link-compat-smoke`, `check-owned-tests`, `package-deb`, debug derivation, and the final dependent harness.
  - Generated checkers from this phase onward run `stage-upstream-build` explicitly before the first `work/original-build` consumer. `check-owned-tests`, `link-compat-smoke`, `package-deb`, and the preserved phase-04/05 wrapper commands may assume that command has already run; they must not silently restage the tree through ad hoc logic.
- Generalize `sync_safe_tests_tree()`, `generate_tests_manifest()`, and `write_generated_test_placeholder()` so later phases can materialize their own `source_path == null` entries instead of silently skipping them. This is required immediately for phase-06-owned generated test assets such as `check-installed-headers-*`, `test-as-const-*`, and compare-output fixtures. Those generated per-subdir `check-installed-headers-*` placeholders live under the owned phase-06 test roots; do not reassign the shared top-level script rows `safe/tests/scripts/check-installed-headers.sh` or `safe/tests/scripts/check-local-headers.sh`, which remain preexisting support assets owned by earlier phases.
- Introduce an owner-phase-aware `cargo run -p xtask -- check-owned-tests ...` verifier.
  - With `--owner-phase <phase-id>`, it must first validate completeness against `safe/generated/baseline/test-port-plan.json`, `safe/generated/baseline/test-catalog.json`, and `safe/tests/manifest.toml`: every catalog row owned by that phase must exist exactly once in the manifest, be marked `ported`, have its committed `safe_path` present on disk, and have every referenced `support_path` present on disk.
  - `--owner-phase <phase-id>` must also verify that the phase’s expected root set is materialized under `safe/tests/**`, including normalized `sysdeps*` destinations and any owned zero-entry destination from `zero_entry_subdirs`. For a zero-entry destination, materialized means the exact committed sentinel `<destination_root>/.gitkeep` exists and is tracked. If any owned executable row is still `planned`, any owned file is missing, or any owned required root or sentinel is absent, the command fails before running tests.
  - After the completeness pass, `--owner-phase <phase-id>` must execute exactly that phase’s owned catalog rows through `run-original-tests` using explicit catalog IDs or an equivalently exact entry selection. It must not infer scope from logical subdir filters.
  - With `--all-ported`, it must fail unless all 5,584 manifest entries are marked `ported`, every referenced committed path exists, and the zero-entry sentinels `safe/tests/nscd/.gitkeep`, `safe/tests/po/.gitkeep`, and `safe/tests/manual/.gitkeep` are present and tracked; only then may it execute the full copied corpus for the final phase without relying on `run-original-tests --families all`.
  - It must treat the shared `safe/tests/scripts/**` catalog rows, `safe/tests/top-level/**`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and normalized `sysdeps*` destinations as exact owned artifacts instead of inferring ownership from logical subdir names.
- Extend `install_root.rs` and `package_deb.rs` so package entries can stage Rust-built DSOs from the active build root.
  - Introduce a source origin or equivalent staging rule for build-time Rust DSOs and for later safe-owned `libc6-dev` compatibility objects or archives.
  - After phase 06, `build/testroot.pristine` may remain only as the source of private backend copies and explicitly inventoried temporary `libc6-dev` startup/static/audit assets that phase 10 has not yet replaced; no public runtime DSO or public `libc6-dev` link-name `.so` may still use `build_testroot`.
  - Every remaining private backend DSO introduced here must be staged under a dedicated non-public path such as `/usr/libexec/safelibs/backends/*.so`, recorded as `asset_kind = "private_baseline_backend_dso"` in `safe/generated/baseline/package-files/*.json`, `safe/generated/install-manifests/*.json`, and `safe/upstream-compat/package-scope.toml`, and recorded as `classification = "private_baseline_backend_dso"` in `safe/generated/baseline/fallback-c-inventory.json`.
  - Continue synthesizing `libpthread_nonshared.a` and preserving the `lib64` mirrors, but record that archive as a generated compatibility asset rather than leaving it implicit outside the manifests.
- Extend `safe/xtask/src/commands/test_package_install.rs` with a `libc-family-cutover` smoke set and stronger installed-package provenance checks.
  - It must inspect `safe/generated/baseline/package-files/libc6.json`, `safe/generated/baseline/package-files/libc6-dev.json`, and `safe/generated/install-manifests/required-packages.json` and fail unless `/usr/lib64/ld-linux-x86-64.so.2`, `/usr/lib64/libc.so.6`, `/usr/lib64/libpthread.so.0`, `/usr/lib64/libthread_db.so.1`, `/usr/lib64/libc_malloc_debug.so.0`, `/usr/lib64/libmemusage.so`, and the corresponding phase-06-owned public `libc6-dev` `.so` link names no longer use `source_origin = "build_testroot"` or any `build/testroot.pristine/lib64/...` public source path.
  - Any remaining `build_testroot` code after phase 06 must be limited to explicitly declared private backend DSOs under `/usr/libexec/safelibs/backends/**`, with matching `private_baseline_backend_dso` entries in the package manifests, install manifests, package-scope ledger, and fallback inventory, plus explicitly inventoried temporary `libc6-dev` startup/static/audit assets; `safe/upstream-compat/package-scope.toml` must mark every such path temporary and assign it to the final cutover.
  - After installing the generated `.deb` packages in the container, the same smoke set must compare the installed public DSOs and phase-06-owned public `libc6-dev` `.so` link names against the staged Rust-built source paths recorded in the manifests using ELF build IDs, hashes, or an equivalently strict provenance check, and it must fail if a public phase-06 DSO still matches a baseline public copy.
  - The same installed-package verifier must also rerun the already-shipped loader/runtime behaviors (`ld.so`, `ldd`, `ldconfig`, and `pldd`) from the installed packages so the libc cutover cannot regress those tools silently.
- `libc-family-cutover` becomes a standing installed-package smoke in this phase. Every later phase that still changes shipped DSOs or installed package payloads must rerun `basic-required-packages`, `libc-family-cutover`, `loader-tools`, and `runtime-tools` in addition to any new phase-specific smoke buckets.
- Remove duplicated hard-coded safe package version literals.
  - Make `package_deb.rs`, `test_package_install.rs`, and `safe/scripts/install-safe-repo.sh` consume the version from `safe/generated/packaging/package-build-manifest.json` or a single shared helper, so package smoke tests and later dependent-harness installs always target the exact package build being produced.
- Add the first committed original-object relink oracle at `safe/generated/baseline/link-compat-corpus.json`.
  - `link-compat-smoke` must fail if this file is missing.
  - Each case entry must record at least `case_id`, `owner_phase`, `coverage_class`, `object_source_kind`, the committed fixture source path when `object_source_kind = "original_sysroot_fixture"`, the harvested or built original object path relative to `safe/work/link-smoke/original-objects/**`, any required startfiles or archives, the exercised public DSOs or symbol families, and whether the relinked binary runs via the safe loader or directly.
  - This file becomes the only authoritative case-selection input for relink coverage. Phase 06 seeds it with the startup-object, ordinary dynamic, PIE, static, static-PIE, and initial `GLIBC_PRIVATE` cases; later phases extend it in place instead of procedurally rescanning `safe/work/original-build/**`.
- Rewrite `safe/xtask/src/commands/link_compat_smoke.rs` from a one-step source-and-link smoke test into a two-stage original-object relink verifier.
  - Stop compiling the compatibility cases directly against `work/install-root`.
  - Add an explicit original-object stage that uses `safe/work/original-build/testroot.pristine` as the original sysroot and writes preserved `.o` artifacts under a dedicated scratch root such as `safe/work/link-smoke/original-objects/`.
  - Prefer harvesting real upstream-built test objects from `safe/work/original-build/**` where they exist as stable standalone `.o` files; for gaps in that committed corpus, compile the fixed fixture sources named in `safe/generated/baseline/link-compat-corpus.json` once against the original sysroot and then treat those `.o` files as the “previously compiled against original libc6” inputs for the relink check.
  - Case discovery must come from `safe/generated/baseline/link-compat-corpus.json`, not from ad hoc directory scans during verification.
  - The relink stage must consume those preserved `.o` files without recompiling them, link them against `work/install-root`, and run the resulting binaries through the safe loader or directly for static cases.
  - Keep the existing coverage classes, but express them in terms of original-built objects: startup-object linkage (beginning with `crt1.o`/`crti.o`/`crtn.o` plus an original-built `main.o`, and expanding by phase 10 to every shipped startfile variant `Mcrt1.o`, `Scrt1.o`, `crt1.o`, `gcrt1.o`, `grcrt1.o`, `rcrt1.o`, `crti.o`, and `crtn.o`), ordinary dynamic linkage, PIE, static linkage, static-PIE linkage, and at least one `GLIBC_PRIVATE` reference case.
- Port the phase-owned libc-family exports into Rust under `safe/crates/libc6/src/**` and `safe/crates/core-runtime/src/**`.
  - Move string/memory/path/time/stdio logic into safe Rust where possible.
  - Keep `unsafe` confined to the C ABI boundary, direct syscalls, TLS/stack/`setjmp` interop, and generated forwarding veneers.
  - Add targeted unit tests for parser and edge-case behavior inside the Rust crates.
- Update the test harness so `run-original-tests` uses the Rust-backed install root for the ported phase-06 surfaces instead of the baseline public DSOs.
- Close or update the CVE rows owned by this cutover:
  - `getrandom / arc4random`
  - `getrandom on powerpc` as either mitigated generically or `not-applicable` for amd64-only shipped code with rationale
  - `realpath`
  - `makecontext / unwinder interop`
  - `strftime`
  - `PTR_MANGLE / pointer guard`

**Verification**

Run `check_06_phase_metadata_backcompat`, then the four `check_06_*` phases in the listed order. The phase is not complete until:

- The legacy phase-04/05 verifier commands still run successfully under their existing CLI names after the phase-metadata refactor.
- `stage-upstream-build` is idempotent, matches the committed phase-05a staging semantics, and every phase-06 checker that consumes `work/original-build` invokes it explicitly before that consumption.
- The public `libc6` package payload and the phase-06-owned public `libc6-dev` `.so` link-name entries no longer point at `build/testroot.pristine/lib64/libc.so.6` or the equivalent baseline public DSO paths for `/usr/lib64/ld-linux-x86-64.so.2`, `/usr/lib64/libpthread.so.0`, `/usr/lib64/libthread_db.so.1`, `/usr/lib64/libc_malloc_debug.so.0`, and `/usr/lib64/libmemusage.so`, and `libc-family-cutover` proves those installed DSOs now come from Rust-built sources.
- Any still-shipped private backend DSO after phase 06 is explicitly listed as `private_baseline_backend_dso` under `/usr/libexec/safelibs/backends/**` in the manifests and ledgers, so later phases can remove that class mechanically instead of rediscovering it.
- `check-abi`, the upgraded original-object `link-compat-smoke`, and `check-headers` all pass against the Rust-backed install root, and `link-compat-smoke` is driven by the committed `safe/generated/baseline/link-compat-corpus.json` oracle rather than by ad hoc source recompilation or directory scanning.
- `check-owned-tests --owner-phase impl_06_io_stdio_string_path` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset passes under the Rust-backed install root.
- Installed `ld.so`, `ldd`, `ldconfig`, and `pldd` continue to pass their package-install smokes after the libc-family cutover.
- Any still-temporary `libc6-dev` startup/static/audit assets remain explicitly inventoried in `safe/upstream-compat/package-scope.toml` as phase-10 obligations.
- `audit-safety` passes at the stronger phase-06 mode with explicit reviewed unsafe/fallback entries.

### 3. NSS, resolver, inet/socket glue, and nscd client/tool cutover

**Implement Phase ID**

`impl_07_nss_resolver_nscd`

**Verification Phases**

- `check_07_network_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_07_nss_resolver_nscd`
  - Purpose: Validate all phase-owned copied test entries and run the upstream network-related tests against the Rust-backed install root.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-owned-tests \
      --owner-phase impl_07_nss_resolver_nscd \
      --root work/install-root \
      --build-root work/original-build \
      --privileged-container-tests
    ```
- `check_07_network_abi`
  - Type: `check`
  - Fixed `bounce_target`: `impl_07_nss_resolver_nscd`
  - Purpose: Verify ABI, versioning, original-object relink compatibility, and runtime linkage for the network-identity DSOs and their libc exports.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --dso libanl --dso libnsl --dso libnss_compat --dso libnss_dns --dso libnss_files --dso libnss_hesiod --dso libresolv --dso libc
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
    ```
- `check_07_network_packages`
  - Type: `check`
  - Fixed `bounce_target`: `impl_07_nss_resolver_nscd`
  - Purpose: Verify cumulative installed-package coverage after the network cutover, including required-package/debug coherence, libc-family payload provenance, earlier loader/runtime entrypoints, and the new `getent`/`nscd`/NSS surfaces.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
    ```
- `check_07_network_safety`
  - Type: `check`
  - Fixed `bounce_target`: `impl_07_nss_resolver_nscd`
  - Purpose: Enforce reviewed unsafe/fallback coverage and CVE disposition for the resolver/NSS/nscd-client surfaces.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --require-cve-disposition
    ```

**Preexisting Inputs**

- All outputs from `impl_06_io_stdio_string_path` and its passing checks.
- Existing authoritative inputs:
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/abi/libanl.json`
  - `safe/generated/baseline/abi/libnsl.json`
  - `safe/generated/baseline/abi/libnss_compat.json`
  - `safe/generated/baseline/abi/libnss_dns.json`
  - `safe/generated/baseline/abi/libnss_files.json`
  - `safe/generated/baseline/abi/libnss_hesiod.json`
  - `safe/generated/baseline/abi/libresolv.json`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/nscd.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `dependents.json`
  - `relevant_cves.json`

**New Outputs**

- Rust-backed `libresolv`, `libanl`, `libnsl`, and `libnss_*` public DSOs or DSO veneers that now run phase-owned symbols in Rust and forward the remainder to private baseline backends.
- A Rust or first-class shipped implementation for `/usr/bin/getent` and `/usr/sbin/nscd`, eliminating those temporary fallback wrappers.
- Ported tests for the 156 phase-owned `hesiod`, `inet`, `nis`, `nss`, `resolv`, `socket`, `safe/tests/sysdeps/**`, and shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` entries.
- Updated network package manifests, install manifests, and closed or dispositioned CVE rows for resolver/NSS/nscd-client issues.

**File Changes**

- Build/package/install:
  - `safe/xtask/src/commands/build.rs`
  - `safe/xtask/src/commands/install_root.rs`
  - `safe/xtask/src/commands/package_deb.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/nscd.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
- Helper tool ownership:
  - `safe/crates/libc-support-tools/src/fallback.rs`
  - new or expanded Rust tool code for `getent`/`nscd`
- Network implementation:
  - new crates or modules for resolver and NSS logic under `safe/crates/**`
  - `safe/Cargo.toml` if new workspace members are added
  - `safe/Cargo.lock`
- Debian/package metadata:
  - `safe/debian/libc-bin.install`
  - `safe/debian/nscd.install`
  - `safe/debian/nscd.init`
  - `safe/debian/nscd.service`
  - `safe/debian/nscd.tmpfiles`
  - `safe/debian/local/etc/nsswitch.conf`
  - `safe/debian/local/etc/nss`
- Tests and ledgers:
  - `safe/tests/manifest.toml`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - phase-owned files under `safe/tests/hesiod/**`, `safe/tests/inet/**`, `safe/tests/nis/**`, `safe/tests/nss/**`, `safe/tests/resolv/**`, `safe/tests/socket/**`, and `safe/tests/sysdeps/**`
  - `safe/tests/nscd/.gitkeep` as the tracked zero-entry sentinel for the `safe/tests/nscd` destination recorded in `safe/generated/baseline/test-port-plan.json`
  - the shared phase-owned script rows at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`

**Implementation Details**

- Extend the phase-06 forwarding-veneer design to the network-identity DSOs and the corresponding libc export set.
- Whenever this phase changes a shipped DSO, NSS module, helper binary, service unit, or configuration-file path, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits so the cumulative package smoke set (`basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, and `network-tools`) verifies the committed authorities.
- Keep the upgraded `safe/xtask/src/commands/link_compat_smoke.rs` authoritative for link compatibility as the network DSOs move off baseline payloads.
  - Extend `safe/generated/baseline/link-compat-corpus.json` in place with the phase-07-owned `libresolv`, `libnss_*`, and `libanl` relink cases, recording each case’s owner phase, object source kind, fixture source when needed, preserved-object path, coverage class, and exercised surfaces.
  - Add or refresh only the committed relink fixtures referenced by that manifest for `libresolv`, `libnss_*`, and `libanl` objects originally built against the original sysroot.
  - Continue requiring that the relink step consume preserved original-built objects instead of recompiling against `work/install-root`.
- Implement resolver parsing and answer validation in Rust, with explicit defenses for:
  - non-answer-section confusion in reverse lookups
  - invalid reverse-DNS hostnames
  - numeric-host parsing corner cases
  - `if_nametoindex`/`getaddrinfo` interaction
  - stub-resolver malformed-message handling
- Replace the library-side nscd client shared-memory reads with a snapshot or generation-checked design that cannot observe torn cross-process state.
- Move `getent` and `nscd` off `RequiredToolKind::FallbackWrapper` ownership for phase 07.
  - `getent` should become a Rust or direct first-class implementation.
  - `nscd` can remain a daemon implemented outside Rust only if the phase makes it a declared, non-temporary shipped asset instead of a hidden fallback wrapper, but the preferred outcome is a Rust-fronted daemon or a direct packaged binary with explicit status.
- Port all phase-owned copied tests and mark them `ported` in `safe/tests/manifest.toml`.
  - This includes the committed `hesiod` and `nis` trees, the normalized `safe/tests/sysdeps/**` destination, the shared `check-wrapper-headers.py` and `check-obsolete-constructs.py` script rows, and the zero-entry sentinel `safe/tests/nscd/.gitkeep` from `zero_entry_subdirs`.
  - Shared script files already exist; phase 07 only updates the phase-owned manifest rows that point at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`.
- Update `cve-status.toml` for:
  - `nss_dns / gethostbyaddr`
  - `nscd client / NSS shared cache`
  - `getaddrinfo numeric host parsing`
  - `getaddrinfo / if_nametoindex`
  - `stub resolver`
  - `NSS files backend`
  - `nss_dns / getnetbyname`
  - `nss_nis / getpwnam`

**Verification**

Run the four `check_07_*` phases in order. Require all of the following before closing phase 07:

- `getent` and `nscd` no longer ship as temporary fallback wrappers in `safe/crates/libc-support-tools/src/fallback.rs`.
- The network DSOs pass `check-abi`.
- The phase-07 relink smoke still uses the phase-06/07 entries committed in `safe/generated/baseline/link-compat-corpus.json` and passes after the `libresolv`/`libnss_*` cutover.
- `check-owned-tests --owner-phase impl_07_nss_resolver_nscd` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset passes under the Rust-backed install root.
- The tracked `safe/tests/nscd/.gitkeep` sentinel remains explicitly accounted for even though that destination has no executable `run-original-tests` coverage.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, and `network-tools`, so required-package/debug coherence and the earlier installed tool surfaces stay live after the network DSO cutover.

### 4. Locale, iconv, localedata, conform, and POSIX parser cutover

**Implement Phase ID**

`impl_08_locale_iconv_posix_parsers`

**Verification Phases**

- `check_08_locale_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_08_locale_iconv_posix_parsers`
  - Purpose: Validate the large phase-owned test tree and run the locale/iconv/POSIX parser tests.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-owned-tests \
      --owner-phase impl_08_locale_iconv_posix_parsers \
      --root work/install-root \
      --build-root work/original-build \
      --privileged-container-tests
    ```
- `check_08_locale_abi`
  - Type: `check`
  - Fixed `bounce_target`: `impl_08_locale_iconv_posix_parsers`
  - Purpose: Verify the libc/libBrokenLocale symbol surface, original-object relink compatibility, and installed headers after the locale/iconv parser cutover.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --dso libc --dso libBrokenLocale
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
    cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
    ```
- `check_08_locale_packages`
  - Type: `check`
  - Fixed `bounce_target`: `impl_08_locale_iconv_posix_parsers`
  - Purpose: Verify cumulative installed-package coverage after the locale cutover, including required-package/debug coherence, earlier libc-family and network payloads, and the new locale/iconv helper scripts and data files.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set locale-tools
    ```
- `check_08_locale_safety`
  - Type: `check`
  - Fixed `bounce_target`: `impl_08_locale_iconv_posix_parsers`
  - Purpose: Enforce CVE disposition and reviewed unsafe/fallback coverage for parsers and locale state machines.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --require-cve-disposition
    ```

**Preexisting Inputs**

- All outputs from `impl_07_nss_resolver_nscd` and its passing checks.
- Existing authoritative inputs:
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/abi/libBrokenLocale.json`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/locales.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/tests/manifest.toml`
  - `relevant_cves.json`

**New Outputs**

- Rust-backed locale/iconv/parser implementations in the public libc surface.
- First-class shipped implementations for:
  - `/usr/bin/iconv`
  - `/usr/sbin/iconvconfig`
  - `/usr/bin/localedef`
  - `/usr/bin/locale`
  - `/usr/sbin/locale-gen`
  - `/usr/sbin/update-locale`
  - `/usr/sbin/validlocale`
  - `/usr/share/locales/install-language-pack`
  - `/usr/share/locales/remove-language-pack`
- Ported test tree for the 1,393 phase-owned test entries.
- Updated locale package manifests, install manifests, and CVE dispositions.

**File Changes**

- Helper tool ownership and packaging:
  - `safe/crates/libc-support-tools/src/fallback.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/debian/libc-bin.install`
  - `safe/debian/locales.install`
  - `safe/debian/locales.config`
  - `safe/debian/locales.postinst`
  - `safe/debian/locales.postrm`
  - `safe/debian/locales.prerm`
  - `safe/debian/local/usr_sbin/*`
  - `safe/debian/local/usr_share_locales/*`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/locales.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/upstream-compat/package-scope.toml`
- Implementation:
  - `safe/crates/libc6/src/**`
  - new locale/iconv/parser crates or modules under `safe/crates/**`
  - generated charset tables or build helpers under `safe/**` as needed
  - `safe/Cargo.toml`
  - `safe/Cargo.lock`
- Tests and ledgers:
  - `safe/tests/manifest.toml`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - phase-owned files under `safe/tests/conform/**`, `safe/tests/posix/**`, `safe/tests/localedata/**`, `safe/tests/iconvdata/**`, `safe/tests/iconv/**`, `safe/tests/locale/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`
  - `safe/tests/po/.gitkeep` as the tracked zero-entry sentinel for the `safe/tests/po` destination recorded in `safe/generated/baseline/test-port-plan.json`
  - the shared phase-owned script rows at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/safety-policy.toml`

**Implementation Details**

- Replace the remaining locale and iconv temporary wrappers with real shipped assets.
  - For true programs (`iconv`, `iconvconfig`, `localedef`, `locale`), prefer Rust executables.
  - For helper scripts (`locale-gen`, `update-locale`, `validlocale`, `install-language-pack`, `remove-language-pack`), ship them directly as first-class scripts instead of routing them through the temporary fallback wrapper mechanism.
- Whenever this phase changes a shipped helper binary, helper script, locale data path, or public package payload entry, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits so the cumulative package smoke set (`basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, and `locale-tools`) verifies the committed authorities.
- Extend `safe/xtask/src/commands/test_package_install.rs` so the `locale-tools` smoke set validates the installed package payload, helper scripts, and locale-maintainer-script behavior from the generated `.deb` outputs rather than from the build tree, while preserving the already-established required-package/debug checks in `basic-required-packages`.
- Port parser-heavy libc logic into safe Rust with explicit malformed-input handling and guaranteed forward progress.
  - Never use assertions for externally reachable malformed input.
  - Preserve glibc error codes, locale search order, and file-format semantics.
- Ensure the generated or loaded locale databases remain package-compatible with the existing Debian maintainer scripts and install paths.
- Extend `safe/generated/baseline/link-compat-corpus.json` in place so it exercises preserved original-built objects or original-sysroot fixtures that reference locale, iconv, and parser-facing public surfaces, including representative `setlocale`/`newlocale`, `iconv`, `regcomp`/`regexec`, `fnmatch`, `glob`, `wordexp`, and `libBrokenLocale` coverage where the original build tree exposes stable object inputs.
- Port the copied tests and keep the `conform` subtree authoritative; do not invent an alternate parser test corpus.
- Carry the `safe/tests/po/.gitkeep` zero-entry sentinel forward exactly as assigned in `test-port-plan.json`, and treat the shared `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py` assets as existing inputs whose phase-owned manifest rows become `ported` in place.
- Update `cve-status.toml` for:
  - all iconv state-machine rows
  - `fnmatch`
  - regex compiler and regex engine rows
  - `wordexp`
  - `glob`
  - `locale path handling`
  - any `crypt` row that is out of package scope, with an explicit `not-applicable` rationale if Ubuntu’s libc6 package no longer ships the relevant implementation

**Verification**

Run the four `check_08_*` phases in order. The phase is complete only when:

- All locale/iconv helper paths owned by phase 08 are no longer temporary fallback binaries.
- The phase-08 relink smoke proves that the phase-06 through phase-08 entries committed in `safe/generated/baseline/link-compat-corpus.json` still link and run against the safe install root after the public libc/libBrokenLocale cutover.
- The locale packages install and run from `.deb` outputs, not just from a build tree.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, and `locale-tools`, so required-package/debug coherence and previously-cut-over installed surfaces stay live after the locale/iconv cutover.
- `check-owned-tests --owner-phase impl_08_locale_iconv_posix_parsers` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset passes under the Rust-backed install root.
- The tracked `safe/tests/po/.gitkeep` sentinel remains explicitly covered even though that destination has no executable tests.

### 5. Math, wide-char, dlfcn/rt/util/debug/sunrpc, and remaining helper-tool cutover

**Implement Phase ID**

`impl_09_math_and_aux_dsos`

**Verification Phases**

- `check_09_math_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_09_math_and_aux_dsos`
  - Purpose: Run the large remaining test corpus for math and auxiliary DSOs.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-owned-tests \
      --owner-phase impl_09_math_and_aux_dsos \
      --root work/install-root \
      --build-root work/original-build \
      --privileged-container-tests
    ```
- `check_09_math_abi`
  - Type: `check`
  - Fixed `bounce_target`: `impl_09_math_and_aux_dsos`
  - Purpose: Verify all remaining auxiliary DSOs, their versioned exports, and original-object relink compatibility.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --dso libdl --dso libm --dso libmvec --dso libpcprofile --dso librt --dso libutil --dso libc
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
    ```
- `check_09_math_packages`
  - Type: `check`
  - Fixed `bounce_target`: `impl_09_math_and_aux_dsos`
  - Purpose: Verify cumulative installed-package coverage after the remaining DSO and helper-tool cutovers, including required-package/debug coherence, prior installed surfaces, and the new dev/time tools.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set locale-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-and-time-tools
    ```
- `check_09_math_safety`
  - Type: `check`
  - Fixed `bounce_target`: `impl_09_math_and_aux_dsos`
  - Purpose: Enforce safety and CVE disposition for the remaining auxiliary runtime surfaces.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --require-cve-disposition
    ```

**Preexisting Inputs**

- All outputs from `impl_08_locale_iconv_posix_parsers` and its passing checks.
- Existing authoritative inputs:
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/abi/libdl.json`
  - `safe/generated/baseline/abi/libm.json`
  - `safe/generated/baseline/abi/libmvec.json`
  - `safe/generated/baseline/abi/libpcprofile.json`
  - `safe/generated/baseline/abi/librt.json`
  - `safe/generated/baseline/abi/libutil.json`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/libc-dev-bin.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/tests/manifest.toml`
  - `relevant_cves.json`

**New Outputs**

- Rust-backed public implementations for the remaining auxiliary DSOs and the math-heavy libc exports.
- First-class shipped implementations for:
  - `/usr/bin/gencat`
  - `/usr/bin/getconf`
  - `/usr/bin/tzselect`
  - `/usr/bin/zdump`
  - `/usr/sbin/zic`
- Ported tests for the remaining 2,088 planned entries.
- Explicit coverage of the remaining phase-owned special assets, including `catgets` message catalogs, the lone `gnulib` test, the shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` manifest rows, and the zero-entry sentinel `safe/tests/manual/.gitkeep`.
- Near-final package manifests and install manifests with only temporary backend DSOs and the explicitly inventoried final `libc6-dev` startup/static/audit cutover work left as phase-10 cleanup.

**File Changes**

- Helper tool ownership and package manifests:
  - `safe/crates/libc-support-tools/src/fallback.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/debian/libc-bin.install`
  - `safe/debian/libc-dev-bin.install`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/libc-dev-bin.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/upstream-compat/package-scope.toml`
- Implementation:
  - new crates or modules for math and auxiliary DSOs under `safe/crates/**`
  - likely activation of `safe/crates/aux-dsos/**` beyond the current README placeholder
  - `safe/Cargo.toml`
  - `safe/Cargo.lock`
  - `safe/xtask/src/commands/build.rs`
  - `safe/xtask/src/commands/install_root.rs`
  - `safe/xtask/src/commands/package_deb.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
- Tests and ledgers:
  - `safe/tests/manifest.toml`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - phase-owned files under `safe/tests/argp/**`, `safe/tests/catgets/**`, `safe/tests/debug/**`, `safe/tests/dlfcn/**`, `safe/tests/gmon/**`, `safe/tests/gnulib/**`, `safe/tests/intl/**`, `safe/tests/login/**`, `safe/tests/math/**`, `safe/tests/mathvec/**`, `safe/tests/resource/**`, `safe/tests/rt/**`, `safe/tests/sunrpc/**`, `safe/tests/sysvipc/**`, `safe/tests/wcsmbs/**`, `safe/tests/wctype/**`, `safe/tests/sysdeps/**`, and `safe/tests/sysdeps-x86_64/**`
  - `safe/tests/manual/.gitkeep` as the tracked zero-entry sentinel for the `safe/tests/manual` destination recorded in `safe/generated/baseline/test-port-plan.json`
  - the shared phase-owned script rows at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/safety-policy.toml`

**Implementation Details**

- Port `libm` and `libmvec` without breaking ABI or performance-sensitive calling conventions.
  - Prefer safe Rust implementations for scalar math logic where feasible.
  - For architecture-specific vector entrypoints, isolate unavoidable assembly or compiler-intrinsic shims in explicit `asm_shim` entries with narrow reviewed unsafe boundaries.
- Whenever this phase changes a shipped helper binary, dev/time tool path, auxiliary DSO payload, or other package-installed asset, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits so the cumulative package smoke set (`basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, `locale-tools`, and `dev-and-time-tools`) verifies the committed authorities.
- Expand the upgraded original-object relink verifier so the phase-09 smoke corpus also covers math and auxiliary DSO references.
  - Extend `safe/generated/baseline/link-compat-corpus.json` in place with original-built object cases that require `libm`, `libdl`, `librt`, and `libutil` symbols, and keep them as preserved `.o` inputs under the relink workflow.
- Port `dlopen`/`dlsym`/`dlvsym`/`dladdr`-related public surfaces consistently with the existing Rust loader control-plane logic from phase 04.
- Replace the remaining helper-tool wrappers owned by phase 09.
- Port the wide-character, message-catalog, timezone, sunrpc, login, and auxiliary runtime surfaces and their tests.
  - Materialize the large set of `source_path == null` phase-09 assets through the generalized placeholder/scaffold path introduced earlier, especially the `catgets` `.cat` fixtures, header-check placeholders, gmon compare files, and other generated test artifacts.
  - Explicitly port the `catgets` and `gnulib` subtrees named in `safe/generated/baseline/test-port-plan.json`, keep the normalized `safe/tests/sysdeps/**` and `safe/tests/sysdeps-x86_64/**` destinations aligned with the manifest, and carry `safe/tests/manual/.gitkeep` forward as the owned zero-entry sentinel for a destination with no executable tests.
  - Shared script files remain single checked-in assets; phase 09 marks only the catalog rows that point at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py` as `ported` in `safe/tests/manifest.toml` instead of duplicating those files.
- Update `cve-status.toml` for the remaining rows owned here, including:
  - `memcmp x32`
  - `sunrpc svc_run`
  - any still-open regex/glob/resource rows that in practice land in this phase’s owned source surface

**Verification**

Run the four `check_09_*` phases in order. The phase is complete only when:

- All phase-09 helper tools are no longer temporary fallback wrappers.
- All remaining non-final DSOs pass `check-abi`.
- The phase-09 relink smoke still proves that the phase-06 through phase-09 entries committed in `safe/generated/baseline/link-compat-corpus.json` link and run against the safe install root.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, `locale-tools`, and `dev-and-time-tools`, so required-package/debug coherence and every earlier installed surface stay live after the final phase-09 DSO and tool cutovers.
- `check-owned-tests --owner-phase impl_09_math_and_aux_dsos` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset, including `catgets`, `gnulib`, `mathvec`, the shared `check-wrapper-headers.py` and `check-obsolete-constructs.py` rows, and the normalized `safe/tests/sysdeps/**` plus `safe/tests/sysdeps-x86_64/**` destinations, passes under the Rust-backed install root.
- The tracked `safe/tests/manual/.gitkeep` sentinel remains explicitly covered even though that destination has no executable tests.

### 6. Final backend removal, package cutover, top-level dependent harness, and full safety closure

**Implement Phase ID**

`impl_10_final_fixup_and_audit`

**Verification Phases**

- `check_10_workspace_unit_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Run the Rust workspace unit and integration tests that sit outside the copied upstream test corpus and package-install smokes.
  - Commands:
    ```bash
    cd safe
    cargo test --workspace
    ```
- `check_10_full_abi_link`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Run full release build, all-DSO ABI verification, header verification, final `libc6-dev` provenance auditing through the relink matrix, and original-object relink verification against the final package payload.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-abi --all-dsos
    cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
    cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
    ```
- `check_10_full_upstream_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Run the full copied upstream test corpus against the final Rust-backed install root.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- check-owned-tests \
      --all-ported \
      --root work/install-root \
      --build-root work/original-build \
      --privileged-container-tests
    ```
- `check_10_full_packages`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Verify the final `.deb` package set through the full smoke matrix, including the phase-06 public DSO provenance cutover check, explicit rejection of shipped private backend DSOs, and the final `libc6-dev` startup/static/dev-link provenance cutover.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cargo run -p xtask -- build --target amd64 --profile release
    cargo run -p xtask -- package-deb --out work/debs
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set backend-payload-closure
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-link-artifacts
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set locale-tools
    cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-and-time-tools
    ```
- `check_10_dependents_dropin`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Verify the actual drop-in replacement goal by installing the safe packages and running the root-level dependent harness.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
    cd ..
    ./safe/scripts/build-debs.sh
    ./test-original.sh
    ```
- `check_10_final_safety`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Enforce the phase-10 strongest safety mode, including zero shipped temporary fallback binaries and zero shipped private baseline backend DSOs.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --deny-shipped-temporary-fallback-binaries \
      --deny-shipped-private-backend-dsos \
      --require-cve-disposition \
      --require-package-scope-clean
    ```

**Preexisting Inputs**

- All outputs from `impl_09_math_and_aux_dsos` and its passing checks.
- Existing authoritative inputs that must now be fully consumed by the final package and harness:
  - `dependents.json`
  - `test-original.sh`
  - `safe/scripts/build-debs.sh`
  - `safe/scripts/install-safe-repo.sh`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`

**New Outputs**

- Final public package payloads with no shipped temporary fallback binaries, no private baseline backend DSOs, and no code-bearing `libc6-dev` startup/static/dev-link asset still sourced from `build_testroot`.
- Updated root-level dependent harness that installs the safe local apt repository before installing and smoke-testing runtime dependents and source-build dependents.
- Final CVE, package-scope, fallback, and safety ledgers with no phase-10 violations.

**File Changes**

- Final backend removal and build enforcement:
  - `safe/xtask/src/commands/audit_safety.rs`
  - `safe/xtask/src/commands/build.rs`
  - `safe/xtask/src/commands/install_root.rs`
  - `safe/xtask/src/commands/package_deb.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/package-files/*.json`
  - `safe/generated/install-manifests/*.json`
  - `safe/generated/packaging/package-build-manifest.json`
- Root-level drop-in harness:
  - `test-original.sh`
  - `safe/scripts/build-debs.sh`
  - `safe/scripts/install-safe-repo.sh`
- Final tool/package metadata:
  - `safe/crates/libc-support-tools/src/fallback.rs`
  - `safe/debian/control`
  - `safe/debian/libc-dev.install`
  - `safe/debian/libc-dev.lintian-overrides`
  - all affected `safe/debian/*.install`, maintainer scripts, and helper files
- Final ledgers:
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/README.md`

**Implementation Details**

- Remove the temporary backend-forwarding mechanism from the shipped package payload.
  - The build must now fail if any shipped public DSO would still need a baseline backend export.
  - Private backend DSOs used during phases 06-09 must disappear from the package manifests, install manifests, package-scope ledger, fallback inventory, and installed package filesystem.
  - Extend `safe/xtask/src/commands/audit_safety.rs` with a dedicated final flag such as `--deny-shipped-private-backend-dsos`. That check must fail if any shipped `safe/upstream-compat/package-scope.toml` entry still has `asset_kind = "private_baseline_backend_dso"` or any shipped `safe/generated/baseline/fallback-c-inventory.json` entry still has `classification = "private_baseline_backend_dso"`.
  - Update `safe/upstream-compat/safety-policy.toml` so the phase-10 `strongest_mode` string includes `--deny-shipped-private-backend-dsos`, and add a fixed policy gate such as `deny_shipped_private_backend_dso_by_phase = "impl_10_final_fixup_and_audit"` so `audit-safety --verify-policy` and the final safety check enforce the same cutoff.
- Finish the `libc6-dev` code-bearing link-asset cutover.
  - `/usr/lib64/{Mcrt1.o,Scrt1.o,crt1.o,crti.o,crtn.o,gcrt1.o,grcrt1.o,rcrt1.o}` may remain assembly-backed on amd64, but they must be emitted from checked-in safe-owned sources such as `safe/crates/compat-asm/x86_64/**` with final manifest provenance `compat_asm`; none may be copied from `build/testroot.pristine`.
  - `/usr/lib64/{libBrokenLocale.a,libanl.a,libc.a,libc_nonshared.a,libdl.a,libg.a,libm-2.39.a,libm.a,libmcheck.a,libmvec.a,libpthread.a,libpthread_nonshared.a,libresolv.a,librt.a,libutil.a}` must be replaced with safe-owned archives or linker scripts whose members come from Rust-built objects plus the minimum documented asm shims. `libpthread_nonshared.a` may remain an empty compatibility archive on amd64, but only if the manifest marks it `synthetic_empty_archive` and `safe/upstream-compat/package-scope.toml` explains the Debian compatibility requirement. If `libg.a`, `libmcheck.a`, or another archive remains a thin compatibility shell rather than a full Rust archive, record that exact rationale in `safe/upstream-compat/package-scope.toml` and `safe/upstream-compat/safety-policy.toml`.
  - Code-bearing `libc6-dev` DSOs and link names such as `/usr/lib64/libc.so`, `/usr/lib64/libm.so`, `/usr/lib64/libBrokenLocale.so`, `/usr/lib64/libanl.so`, `/usr/lib64/libc_malloc_debug.so`, `/usr/lib64/libmvec.so`, `/usr/lib64/libnss_compat.so`, `/usr/lib64/libnss_hesiod.so`, `/usr/lib64/libresolv.so`, `/usr/lib64/libthread_db.so`, and `/usr/lib64/audit/sotruss-lib.so` must resolve to safe-built artifacts or symlinks to safe-built DSOs; none may keep `source_origin = "build_testroot"`.
  - The only final `build_testroot` entries still permitted in `safe/generated/baseline/package-files/libc6-dev.json` are non-code headers, generated-but-data-only files, or documentation or debug helpers whose `asset_kind`, `safe/upstream-compat/package-scope.toml`, and `safe/upstream-compat/safety-policy.toml` entries mark them non-executable and explain why copying them is acceptable.
- Update `test-original.sh` so it actually tests the safe port.
  - `safe/scripts/build-debs.sh` must explicitly run `cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build` and then `cargo run -p xtask -- build --target amd64 --profile release` before `package-deb`, unless `package-deb` itself is upgraded to enforce those same two prerequisites internally. The final workflow must not assume the ignored staged upstream tree already exists.
  - Keep the Docker-based dependent smoke model.
  - Inside the container, call `safe/scripts/install-safe-repo.sh` before installing `libc6`, `libc6-dev`, `libc6-dbg`, `libc-bin`, `libc-dev-bin`, `locales`, and `nscd`.
  - Preserve the current 16 runtime-dependent smoke checks and 3 source-build checks for `strace`, `valgrind`, and `libvirt`.
  - Keep the source-repository setup from the current script so source builds still happen inside Ubuntu 24.04.
- Make the package-install and dependent-harness path use the same package version source.
  - `safe/scripts/install-safe-repo.sh`, `safe/xtask/src/commands/test_package_install.rs`, and `test-original.sh` must derive or receive the expected package version from `safe/generated/packaging/package-build-manifest.json` or the same shared helper used by `package_deb.rs`.
- Extend `safe/xtask/src/commands/test_package_install.rs` with a final `dev-link-artifacts` smoke set.
  - It must inspect `safe/generated/baseline/package-files/libc6-dev.json`, `safe/generated/install-manifests/required-packages.json`, and `safe/upstream-compat/package-scope.toml`, fail if any final code-bearing `libc6-dev` asset still uses `build_testroot` provenance, and byte-compare installed `.o`, `.a`, `.so`, and audit-helper payloads against the safe-owned `source_path` entries recorded in the manifests.
  - Inside the container, it must compile and run at least one dynamic, PIE, static, and static-PIE sample against the installed `libc6-dev` payload, and it must explicitly exercise every shipped startfile variant (`Mcrt1.o`, `Scrt1.o`, `crt1.o`, `gcrt1.o`, `grcrt1.o`, `rcrt1.o`, `crti.o`, and `crtn.o`) either through compiler-driver flags or direct linker invocations.
- Extend `safe/xtask/src/commands/test_package_install.rs` with a final `backend-payload-closure` smoke set.
  - It must inspect `safe/generated/baseline/package-files/*.json`, `safe/generated/install-manifests/required-packages.json`, `safe/generated/install-manifests/test-install-root.json`, `safe/upstream-compat/package-scope.toml`, and `safe/generated/baseline/fallback-c-inventory.json`, and fail if any shipped entry still uses `asset_kind = "private_baseline_backend_dso"`, `classification = "private_baseline_backend_dso"`, or a shipped path under `/usr/libexec/safelibs/backends/**`.
  - Inside the container, after installing the final packages, it must fail if any backend DSO remains installed under `/usr/libexec/safelibs/backends/**` or if any non-public DSO payload still byte-matches a manifest entry marked `private_baseline_backend_dso`.
- Finalize the upgraded link-compat verifier against the no-backend end state.
  - `safe/xtask/src/commands/link_compat_smoke.rs` must now fail if any relinked original-built object still resolves through a temporary backend payload, requires recompilation against `work/install-root`, or uses a final `libc6-dev` startfile or static archive that still traces to `build_testroot`.
  - The final smoke corpus must be the committed cumulative `safe/generated/baseline/link-compat-corpus.json`. By the end of phase 10 it must cover dynamic, PIE, static, static-PIE, startup-object, profiling-startfile, and `GLIBC_PRIVATE` cases using preserved original-built `.o` inputs, and it should include representative original test objects from the staged upstream build where available.
- Clean up package metadata so the final package set is internally consistent and still limited to the required seven-package surface unless a checker proves expansion is necessary.
- Close every remaining open CVE row or mark it `not-applicable` with a concrete package-scope rationale.
- Ensure the final safety policy’s strongest mode passes without exceptions.

**Verification**

Run the six `check_10_*` phases in order. Phase 10 is not complete until all of the following are true:

- `cargo test --workspace` passes under `check_10_workspace_unit_tests`.
- `check-abi --all-dsos` passes.
- `check-owned-tests --all-ported` proves that all 5,584 catalog rows are marked `ported`, the zero-entry sentinels are present, and the executable corpus passes.
- Every `.deb` smoke bucket passes, including `dev-link-artifacts`.
- `backend-payload-closure` passes, proving the manifests, ledgers, and installed package filesystem no longer ship any `private_baseline_backend_dso` payload.
- The updated root-level `./test-original.sh` passes end-to-end.
- `audit-safety` passes with both `--deny-shipped-temporary-fallback-binaries` and `--deny-shipped-private-backend-dsos`.
- `link-compat-smoke` passes by relinking the cumulative committed cases from `safe/generated/baseline/link-compat-corpus.json`, not by recompiling them against the safe headers or DSOs, and it fails if any final `libc6-dev` startfile or static archive still comes from `build_testroot`.
- No shipped package or install manifest row, package-scope entry, fallback-inventory entry, or installed filesystem path remains under `/usr/libexec/safelibs/backends/**`.
- The final `libc6-dev` manifest contains no code-bearing `build_testroot` entries; any remaining `build_testroot` rows are non-code and explicitly justified in `safe/upstream-compat/package-scope.toml` and `safe/upstream-compat/safety-policy.toml`.

## Critical Files

The following files and directories are the critical touch points for the remaining implementation. Paths are grouped when many files in the same subtree will move together.

### Read-only Authorities

- `original/**`
  - Full upstream glibc 2.39 source, makefiles, tests, and headers. Read-only oracle for behavior, exported APIs, tests, and packaging expectations.
- `dependents.json`
  - Authoritative dependent inventory. Consume in place.
- `relevant_cves.json`
  - Authoritative non-memory CVE selection. Consume in place.

### Build, Phase, and Verification Orchestration

- `safe/Cargo.toml`
  - Workspace membership; add future DSO crates here if needed.
- `safe/Cargo.lock`
  - Lock any new crate dependencies introduced by phases 06-10.
- `safe/xtask/src/common.rs`
  - Replace the phase-05-only frontier constants with data-driven metadata and keep package/test manifest types authoritative.
- `safe/xtask/src/main.rs`
  - Register the new staged-upstream-build and owner-phase-aware test-checker commands while preserving the existing `check_04_*` and `check_05_*` subcommand names.
- `safe/xtask/src/commands/mod.rs`
  - Wire new checker modules without dropping the legacy phase-04/05 verifier modules.
- `safe/xtask/src/commands/ingest_baseline.rs`
  - Source of the existing upstream-build staging logic that phase 06 should factor into a dedicated reusable command without rewriting checked-in baselines.
- `safe/xtask/src/commands/stage_upstream_build.rs`
  - Dedicated creation or validation path for the staged upstream build tree under `safe/work/original-build/**`; phase 06 adopts any phase-05a-recreated tree in place and must share the exact staging semantics committed in `safe/scripts/stage-original-build.sh`.
- `safe/scripts/stage-original-build.sh`
  - Phase-05a committed helper that stages or validates `safe/work/original-build/**` from `original/**` using the fixed safe-baseline install layout without rewriting committed authorities. Phase 06 `stage-upstream-build` must call or mirror this logic exactly.
- `safe/xtask/src/commands/check_04_loader_tests.rs`, `check_04_loader_abi.rs`, `check_04_loader_tools.rs`, `check_05_core_runtime_tests.rs`, `check_05_core_runtime_abi.rs`, `check_05_runtime_tools.rs`, `check_05_base_dependent_smoke.rs`
  - Historical verifier entrypoints that phase 06 must keep semantically stable as thin wrappers over the new phase-aware helpers.
- `safe/xtask/src/commands/build.rs`
  - Most important refactor point: convert stub-only hybrid shells into real forwarding/public DSO builds, generalize phase refresh logic, and stop phase-05-specific document rewriting.
- `safe/xtask/src/commands/install_root.rs`
  - Materialize final Rust-built payloads, private backend copies for phases 06-09, and final package-like install roots.
- `safe/xtask/src/commands/package_deb.rs`
  - Stage real Rust-built DSOs and tools into `.deb` payloads.
- `safe/xtask/src/commands/check_owned_tests.rs`
  - Select exact phase-owned or all-ported executable test entries from the committed ownership/status ledgers and run them through the upstream-compatible harness without relying on ambiguous logical subdir filters.
- `safe/xtask/src/commands/run_original_tests.rs`
  - Core upstream-compatible runner that `check_owned_tests.rs` must drive with exact entry selections instead of broad logical-subdir guesses.
- `safe/xtask/src/commands/link_compat_smoke.rs`
  - Replace the current source-based smoke test with a two-stage relink verifier that preserves original-built `.o` inputs from `safe/work/original-build/**` or an original-sysroot fixture build, then links and runs those objects against the safe install root. It must take its case selection from the committed `safe/generated/baseline/link-compat-corpus.json` oracle rather than from ad hoc scans. By phase 10 it must also audit final `libc6-dev` provenance for every shipped startfile and static-link asset it exercises.
- `safe/xtask/src/commands/check_headers.rs`
  - Preserve source-compatibility by compiling representative installed headers and validating wrapper headers.
- `safe/xtask/src/commands/test_package_install.rs`
  - Extend smoke-set coverage beyond the current `basic-required-packages`, `loader-tools`, and `runtime-tools` buckets. Phase 06 must add `libc-family-cutover` to prove manifest plus installed-payload provenance for `/usr/lib64/{ld-linux-x86-64.so.2,libc.so.6,libpthread.so.0,libthread_db.so.1,libc_malloc_debug.so.0,libmemusage.so}` and the matching public `libc6-dev` `.so` link names, and rerun installed `ld.so`/`ldd`/`ldconfig`/`pldd` behavior. Package-smoke coverage becomes cumulative at that point: phase 07 must rerun `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, and add `network-tools`; phase 08 must rerun all of those and add `locale-tools`; phase 09 must rerun all of those and add `dev-and-time-tools`; phase 10 must rerun every previously introduced smoke set and add `dev-link-artifacts`. `basic-required-packages` remains the required installed-package and detached-debug-package coherence proof unless a later stricter smoke explicitly subsumes it, so no later phase that still changes shipped DSOs or installed package payloads may drop it. `dev-link-artifacts` must prove that installed `libc6-dev` startup objects, static archives, audit helpers, and development-link DSOs no longer come from `build_testroot` and still compile, link, and run correctly. Every smoke set must use the same package-version source as `package_deb.rs`.
- `safe/xtask/src/commands/audit_safety.rs`
  - Keep the safety-policy schema authoritative and make final phase checks strict.
- `safe/work/original-build/**`
  - Persistent staged upstream build tree consumed by package staging, debug derivation, link-compat relinks, and copied upstream test execution. Phase 05a may recreate it locally if absent so the landed baseline can be revalidated; phase 06 then formalizes and validates that path and later phases reuse it in place.
- `safe/upstream-tests/build/**`
  - Disposable harness scratch used by `run-original-tests`; do not confuse it with `safe/work/original-build/**`.

### Rust Runtime and DSO Implementation

- `safe/crates/libc6/src/**`
  - Main libc ABI export surface and owned subsystem logic.
- `safe/crates/core-runtime/src/**`
  - Low-level syscall, TLS, allocator, signal, and entropy support.
- `safe/crates/libpthread/src/**`
  - Thread state and synchronization behavior required by libc-family exports.
- `safe/crates/libthread-db/src/**`
  - Debugger-facing thread database compatibility.
- `safe/crates/ldso/src/**`
  - Already-ported loader control-plane logic that later `dl*` and runtime cutovers must stay consistent with.
- `safe/crates/libc-support-tools/src/**`
  - Current helper-tool frontend/fallback map; phases 07-09 remove the remaining temporary wrappers from here.
- `safe/crates/compat-asm/x86_64/**`
  - Add the minimum unavoidable forwarding veneers, startup shims, and versioned-export assembly here.
- Any new DSO crates added under `safe/crates/**`
  - Likely candidates: resolver/NSS, locale/iconv, math, and auxiliary DSO crates.

### Package and Install Manifests

- `safe/generated/baseline/package-files/libc6.json`
  - Switch public DSOs from baseline payload copies to Rust-built artifacts in phase order.
- `safe/generated/baseline/package-files/libc-bin.json`
  - Replace fallback helper entrypoints with Rust or first-class shipped implementations.
- `safe/generated/baseline/package-files/libc6-dev.json`
  - Track the full `libc6-dev` cutover explicitly. Headers may remain copied or generated data, but every code-bearing startup object, static archive, public `.so` link name, audit helper, and `libpthread_nonshared.a` entry must end phase 10 with safe-owned provenance rather than `build_testroot`.
- `safe/generated/baseline/package-files/libc6-dbg.json`
  - Keep detached debug companions aligned with the final Rust-built ELF payloads.
- `safe/generated/baseline/package-files/libc-dev-bin.json`
  - Phase-09 helper tool cutover.
- `safe/generated/baseline/package-files/locales.json`
  - Phase-08 locale data and helper cutover.
- `safe/generated/baseline/package-files/nscd.json`
  - Phase-07 daemon/tool cutover.
- `safe/generated/install-manifests/required-packages.json`
  - Authoritative shipped install-root view; update in place.
- `safe/generated/install-manifests/test-install-root.json`
  - Authoritative test-only install-root view; update in place.
- `safe/generated/packaging/package-build-manifest.json`
  - Package builder input; keep package set and helper assets aligned.

### Debian Packaging Surface

- `safe/debian/control`
  - Final package metadata for the seven required packages.
- `safe/debian/libc-dev.install`
  - Exact installed `libc6-dev` payload contract for headers, startfiles, static archives, `.so` link names, audit helpers, and `libpthread_nonshared.a`; the final phase must not leave code-bearing `/usr/lib*/**` entries on `build_testroot`.
- `safe/debian/*.install`, `safe/debian/*.dirs`, `safe/debian/*.links`, `safe/debian/*.symbols.amd64`
  - Keep package payload declarations aligned with the staged Rust-built artifacts.
- `safe/debian/libc*.postinst`, `*.preinst`, `*.postrm`, `locales.*`, `nscd.*`
  - Final package installation and service behavior.
- `safe/debian/local/**`
  - Locale helper scripts, manpages, `ldconfig` local assets, and configuration files.

### Tests, Status, and Safety Ledgers

- `safe/generated/baseline/test-catalog.json`
  - Read-only test inventory oracle.
- `safe/generated/baseline/test-port-plan.json`
  - Read-only ownership oracle; do not regenerate.
- `safe/generated/baseline/committed-safe-frontier.txt`
  - Phase-05a committed manifest of every intentionally tracked non-derived file under `safe/**` after cache cleanup. Later phases treat it as the mechanical landing record rather than regenerating it.
- `safe/generated/baseline/link-compat-corpus.json`
  - Committed original-object relink oracle seeded in phase 06 and extended in place afterward. Every relink verifier must consume this file instead of inventing or rescanning coverage cases procedurally.
- `safe/tests/manifest.toml`
  - Ported/planned status for every test entry; update in place.
- `safe/tests/support/**`
  - Shared upstream support subtree.
- `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`
  - Shared harness inputs copied once from upstream; later phases must consume them in place and `check-owned-tests` must verify referenced files still exist.
- `safe/tests/scripts/check-c++-types.sh`, `check-execstack.awk`, `check-initfini.awk`, `check-installed-headers.sh`, `check-local-headers.sh`, `check-localplt.awk`, `check-obsolete-constructs.py`, `check-textrel.awk`, `check-wrapper-headers.py`, `check-wx-segment.py`, `lint-makefiles.sh`
  - Shared phase-owned special-test assets already committed in the tree; later phases should update manifest rows in place instead of creating duplicate script copies.
- `safe/tests/top-level/Makefile`, `Makeconfig`, `Makerules`
  - Upstream-compatible top-level test harness files.
- `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, `safe/tests/sysdeps-linux-x86_64/**`
  - Concrete normalized destinations for catalog subdir `sysdeps`; later phases must name the right on-disk roots, not just the logical subdir.
- `safe/tests/nscd/.gitkeep`, `safe/tests/po/.gitkeep`, `safe/tests/manual/.gitkeep`
  - Tracked zero-entry sentinels for the destinations from `safe/generated/baseline/test-port-plan.json`; each sentinel is the committed proof that its root exists even though `run-original-tests` will not execute that destination.
- `safe/tests/**`
  - Phase-owned copied tests and assets under the subdirectories assigned by `test-port-plan.json`.
- `safe/generated/baseline/fallback-c-inventory.json`
  - Single source of truth for non-Rust assets and temporary backends; phase 10 must eliminate shipped temporary fallback binaries.
- `safe/upstream-compat/port-status.toml`
  - High-level subsystem and DSO status ledger.
- `safe/upstream-compat/package-scope.toml`
  - Package ownership, helper paths, and shipped-asset classifications.
- `safe/upstream-compat/cve-status.toml`
  - Required CVE disposition ledger.
- `safe/upstream-compat/safety-policy.toml`
  - Required unsafe/fallback review policy and final strongest-mode contract.

### Root-Level Drop-In Harness

- `test-original.sh`
  - Final dependent harness must install and test the safe `.deb` packages, not stock Ubuntu libc6.
- `safe/scripts/build-debs.sh`
  - Build helper for the local safe apt repo; it must first validate or materialize `safe/work/original-build/**`, then package a current release build rather than stale artifacts.
- `safe/scripts/install-safe-repo.sh`
  - In-container local apt repository bootstrap and package pinning; it must consume the same safe package version that `package_deb.rs` emits.

## Final Verification

After `impl_10_final_fixup_and_audit` lands, verify the full implementation with the end-to-end sequence below. It mirrors the explicit `check_10_*` verifiers and starts by validating or materializing the ignored staged upstream build tree under `safe/work/original-build/**` through the dedicated phase-06 helper instead of assuming that tree is already present:

```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cargo test --workspace
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check-abi --all-dsos
cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
cargo run -p xtask -- check-owned-tests \
  --all-ported \
  --root work/install-root \
  --build-root work/original-build \
  --privileged-container-tests
cargo run -p xtask -- package-deb --out work/debs
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set basic-required-packages
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set libc-family-cutover
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set backend-payload-closure
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-link-artifacts
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set loader-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set runtime-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set locale-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-and-time-tools
cd ..
./safe/scripts/build-debs.sh
./test-original.sh
cd safe
cargo run -p xtask -- audit-safety \
  --deny-unreviewed-unsafe \
  --deny-untracked-fallback-c \
  --deny-shipped-temporary-fallback-binaries \
  --deny-shipped-private-backend-dsos \
  --require-cve-disposition \
  --require-package-scope-clean
```

In this sequence, `link-compat-smoke` is the upgraded relink verifier described above: it must consume the cumulative committed cases from `safe/generated/baseline/link-compat-corpus.json`, materialize or validate the preserved objects built against the original libc6, and fail if it has to recompile those cases against `work/install-root`.

Acceptance criteria for the whole port:

- All public headers, startup objects, static archives, and development link-name DSOs still compile existing C consumers, and no final code-bearing `libc6-dev` entry remains sourced from `build_testroot`.
- All 20 DSO baselines pass symbol/version/SONAME verification.
- Previously compiled objects still link and run against the final install root, and that result is proven by relinking the preserved original-built `.o` cases recorded in `safe/generated/baseline/link-compat-corpus.json` rather than recompiling sources against the safe install root.
- The Rust workspace unit and integration test layer passes under `cargo test --workspace`.
- The full copied upstream test corpus passes.
- The safe `.deb` packages install cleanly on Ubuntu 24.04, satisfy every package smoke set including `backend-payload-closure`, prove that the phase-06 public libc-family DSOs and the final code-bearing `libc6-dev` link assets come from safe-built or explicitly justified compatibility sources rather than `build_testroot` public copies, and leave no shipped manifest row or installed filesystem path under `/usr/libexec/safelibs/backends/**`.
- The updated `test-original.sh` passes all 16 runtime-dependent checks and all 3 source-build checks while using the safe package set.
- `audit-safety` passes in final strongest mode with zero shipped temporary fallback binaries, zero shipped private baseline backend DSOs, and explicit disposition for every relevant CVE.
