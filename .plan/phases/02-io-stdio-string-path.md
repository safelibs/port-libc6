# Phase Name
I/O, Stdio, String, Path, Time, and First Libc-Family Cutover

## Implement Phase ID
`impl_06_io_stdio_string_path`

## Preexisting Inputs
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
  - `safe/generated/security/relevant-cves-index.json`
  - `safe/upstream-compat/*.toml`
  - `safe/scripts/stage-original-build.sh`
- Existing authoritative inputs that phase 06 must extend in place rather than rediscover or regenerate:
  - `original/**`
  - `relevant_cves.json`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/test-catalog.json`
- Existing copied-test roots already materialized before phase 06 and touched again here must be updated in place rather than rematerialized:
  - `safe/tests/libio/**`
  - `safe/tests/stdlib/**`
  - `safe/tests/sysdeps/**`
  - `safe/tests/sysdeps-x86_64/**`
  - `safe/tests/sysdeps-linux-x86_64/**`
- Existing shared harness inputs already materialized on disk and still authoritative, especially `safe/tests/support/**`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets under `safe/tests/scripts/**`, including `safe/tests/scripts/check-installed-headers.sh` and `safe/tests/scripts/check-local-headers.sh`.
- Preserve the consume-existing-artifacts contract from phase `05a`: do not mutate `original/**`, do not replace `safe/generated/baseline/test-port-plan.json`, and treat already-materialized shared test assets as authoritative inputs.

## New Outputs
- A dedicated `stage-upstream-build` `xtask` path that mirrors the committed `safe/scripts/stage-original-build.sh` semantics and can create or validate the persistent staged upstream build tree under `safe/work/original-build/**`, including `testroot.pristine`; if phase `05a` already recreated that tree locally, phase 06 adopts it in place instead of rebuilding it.
- A committed relink oracle at `safe/generated/baseline/link-compat-corpus.json` that fixes the initial original-object relink matrix for later phases instead of letting them rediscover coverage ad hoc.
- A real Rust-backed build path for the libc-family DSOs, with generated forwarding veneers instead of zero-return stubs.
- Updated package manifests, install manifests, package-scope entries, and installed-package cutover smokes that point the shipped `libc6` payload and the `libc6-dev` public `.so` link names at Rust-built public DSOs plus private baseline backend copies, while explicitly inventorying any still-temporary `libc6-dev` startup, static, or audit artifacts that remain on `build_testroot` until phase 10.
- Ported test sources for the 654 phase-owned entries covering `stdio-common`, `stdlib`, `libio`, `string`, `io`, `time`, `dirent`, `assert`, `ctype`, `termios`, `timezone`, the shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` rows, the generated per-subdir `check-installed-headers-*` placeholders that live under those owned roots, and the normalized `safe/tests/{sysdeps,sysdeps-x86_64,sysdeps-linux-x86_64}/**` destinations.
- Updated safety and CVE status ledgers for the runtime, path, and time issues that stop being `baseline backend` exceptions once the public libc-family DSOs switch over.

## File Changes
- Phase metadata and staging orchestration:
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
- Build, install, and package cutover:
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
- Tests and ledgers:
  - `safe/tests/manifest.toml`
  - phase-owned files under `safe/tests/stdio-common/**`, `safe/tests/stdlib/**`, `safe/tests/libio/**`, `safe/tests/string/**`, `safe/tests/io/**`, `safe/tests/time/**`, `safe/tests/dirent/**`, `safe/tests/assert/**`, `safe/tests/ctype/**`, `safe/tests/termios/**`, `safe/tests/timezone/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`, including generated `check-installed-headers-*`, `test-as-const-*`, compare-output fixtures, and other `source_path == null` placeholders assigned to those roots
  - the shared phase-owned script rows at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`, whose manifest rows must be marked `ported` in place without duplicating the files
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
  - `safe/README.md`

## Implementation Details
- Replace the current stub-only hybrid-shell generator in `safe/xtask/src/commands/build.rs` with a real incremental DSO builder that uses the checked-in ABI JSON and version scripts as the export oracle.
- Generate x86_64 assembly veneers for every exported symbol still missing from the Rust object set for a DSO. Resolve forwarded symbols by exact version with `dlvsym`, not ordinary `dlsym`, and bind versioned public names with `.symver`.
- Make the build fail if a Rust-provided symbol collides with a generated veneer name or if a baseline export cannot be resolved in the private backend copy.
- Replace phase-05-specific refresh logic with explicit per-command owner metadata and a data-driven frontier list. Preserve the existing CLI names `check_04_loader_tests`, `check_04_loader_abi`, `check_04_loader_tools`, `check_05_core_runtime_tests`, `check_05_core_runtime_abi`, `check_05_runtime_tools`, and `check_05_base_dependent_smoke`.
- Keep the legacy checker modules as thin wrappers with fixed owner phase, DSO set, helper-path assertions, and install-root defaults.
- `check_04_loader_tests.rs` and `check_05_core_runtime_tests.rs` must continue enforcing their current extra invariants after the refactor: the normalized `sysdeps*` destination expectations for phase 04 and the phase-05 stdlib allowlist for `tst-arc4random-*` plus `tst-getrandom`.
- `safe/xtask/src/main.rs` and `safe/xtask/src/commands/mod.rs` must continue registering those legacy subcommands under the same names while also adding `stage-upstream-build` and `check-owned-tests`.
- Stop hard-coding phase-05 prose into generated READMEs and manifest notes.
- Preserve already-committed later-phase `safe/tests/manifest.toml` status changes in place instead of regenerating every post-phase-05 row back to `planned` from `COMPLETED_PHASES`.
- Introduce `stage-upstream-build` without inventing a second build recipe. It must call or mirror the committed `safe/scripts/stage-original-build.sh` validation list, `configparms`, `configure` flags, and `make testroot.pristine/install.stamp` sequence exactly.
- If `impl_05a_commit_prepared_safe_frontier` already recreated `safe/work/original-build/**`, validate and adopt that tree in place. Otherwise create it exactly once here and treat it as persistent workspace state for phases 07-10, `link-compat-smoke`, `check-owned-tests`, `package-deb`, debug derivation, and the final dependent harness.
- Generated checkers from this phase onward must run `stage-upstream-build` explicitly before the first `work/original-build` consumer. `check-owned-tests`, `link-compat-smoke`, `package-deb`, and the preserved phase-04 and phase-05 wrapper commands may assume that command has already run; they must not silently restage the tree through ad hoc logic.
- Generalize `sync_safe_tests_tree()`, `generate_tests_manifest()`, and `write_generated_test_placeholder()` so later phases can materialize their `source_path == null` entries instead of silently skipping them. This is required immediately for phase-06-owned generated test assets such as `check-installed-headers-*`, `test-as-const-*`, and compare-output fixtures.
- Those generated per-subdir `check-installed-headers-*` placeholders live under the owned phase-06 test roots; do not reassign the shared top-level script rows `safe/tests/scripts/check-installed-headers.sh` or `safe/tests/scripts/check-local-headers.sh`, which remain preexisting support assets owned by earlier phases.
- Introduce `cargo run -p xtask -- check-owned-tests` with owner-phase-aware completeness checks against `safe/generated/baseline/test-port-plan.json`, `safe/generated/baseline/test-catalog.json`, and `safe/tests/manifest.toml`.
- With `--owner-phase <phase-id>`, it must validate completeness first: every catalog row owned by that phase must exist exactly once in the manifest, be marked `ported`, have its committed `safe_path` present on disk, and have every referenced `support_path` present on disk.
- `--owner-phase <phase-id>` must also verify that the phase’s expected root set is materialized under `safe/tests/**`, including normalized `sysdeps*` destinations and any owned zero-entry destination from `zero_entry_subdirs`. For a zero-entry destination, materialized means the exact committed sentinel `<destination_root>/.gitkeep` exists and is tracked. If any owned executable row is still `planned`, any owned file is missing, or any owned required root or sentinel is absent, the command fails before running tests.
- After the completeness pass, `--owner-phase <phase-id>` must execute exactly that phase’s owned catalog rows through `run-original-tests` using explicit catalog IDs or an equivalently exact entry selection. It must not infer scope from logical subdir filters.
- With `--all-ported`, it must fail unless all 5,584 manifest entries are marked `ported`, every referenced committed path exists, and the zero-entry sentinels `safe/tests/nscd/.gitkeep`, `safe/tests/po/.gitkeep`, and `safe/tests/manual/.gitkeep` are present and tracked; only then may it execute the full copied corpus for the final phase without relying on `run-original-tests --families all`.
- It must treat the shared `safe/tests/scripts/**` catalog rows, `safe/tests/top-level/**`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and normalized `sysdeps*` destinations as exact owned artifacts instead of inferring ownership from logical subdir names.
- Extend `install_root.rs` and `package_deb.rs` so package entries can stage Rust-built DSOs from the active build root and track private backend DSOs as `asset_kind = "private_baseline_backend_dso"` under `/usr/libexec/safelibs/backends/**`.
- Introduce a source origin or equivalent staging rule for build-time Rust DSOs and for later safe-owned `libc6-dev` compatibility objects or archives so the manifests distinguish active-build safe artifacts from the temporary `build_testroot` carryovers that remain phase-10 obligations.
- After phase 06, `build/testroot.pristine` may remain only as the source of private backend copies and explicitly inventoried temporary `libc6-dev` startup, static, or audit assets that phase 10 has not yet replaced; no public runtime DSO or public `libc6-dev` link-name `.so` may still use `build_testroot`.
- Continue synthesizing `libpthread_nonshared.a` and preserving the `lib64` mirrors, but record that archive as a generated compatibility asset rather than leaving it implicit outside the manifests.
- Extend `test_package_install.rs` with a `libc-family-cutover` smoke set that proves the phase-06-owned public runtime DSOs and public `libc6-dev` `.so` link names no longer use `build_testroot` provenance and that the installed package payload matches the staged Rust-built sources.
- It must inspect `safe/generated/baseline/package-files/libc6.json`, `safe/generated/baseline/package-files/libc6-dev.json`, and `safe/generated/install-manifests/required-packages.json` and fail unless `/usr/lib64/ld-linux-x86-64.so.2`, `/usr/lib64/libc.so.6`, `/usr/lib64/libpthread.so.0`, `/usr/lib64/libthread_db.so.1`, `/usr/lib64/libc_malloc_debug.so.0`, `/usr/lib64/libmemusage.so`, and the corresponding phase-06-owned public `libc6-dev` `.so` link names no longer use `source_origin = "build_testroot"` or any `build/testroot.pristine/lib64/...` public source path.
- Any remaining `build_testroot` code after phase 06 must be limited to explicitly declared private backend DSOs under `/usr/libexec/safelibs/backends/**`, with matching `private_baseline_backend_dso` entries in the package manifests, install manifests, package-scope ledger, and fallback inventory, plus explicitly inventoried temporary `libc6-dev` startup, static, or audit assets; `safe/upstream-compat/package-scope.toml` must mark every such path temporary and assign it to the final cutover.
- After installing the generated `.deb` packages in the container, the same smoke set must compare the installed public DSOs and phase-06-owned public `libc6-dev` `.so` link names against the staged Rust-built source paths recorded in the manifests using ELF build IDs, hashes, or an equivalently strict provenance check, and it must fail if a public phase-06 DSO still matches a baseline public copy.
- The same installed-package verifier must also rerun the already-shipped loader/runtime behaviors (`ld.so`, `ldd`, `ldconfig`, and `pldd`) from the installed packages so the libc cutover cannot regress those tools silently.
- `libc-family-cutover` becomes a standing installed-package smoke in this phase. Every later phase that still changes shipped DSOs or installed package payloads must rerun `basic-required-packages`, `libc-family-cutover`, `loader-tools`, and `runtime-tools` in addition to any new phase-specific smoke buckets.
- Remove duplicated hard-coded safe package version literals by making `package_deb.rs`, `test_package_install.rs`, and `safe/scripts/install-safe-repo.sh` consume the version from `safe/generated/packaging/package-build-manifest.json` or a single shared helper.
- Seed `safe/generated/baseline/link-compat-corpus.json` and rewrite `link_compat_smoke.rs` into a two-stage original-object relink verifier that preserves objects built against the original sysroot, then relinks and runs them against `work/install-root` without recompiling them against the safe headers or DSOs.
- `link-compat-smoke` must fail if this file is missing, and `safe/generated/baseline/link-compat-corpus.json` becomes the only authoritative case-selection input for relink coverage.
- Each case entry in `safe/generated/baseline/link-compat-corpus.json` must record at least `case_id`, `owner_phase`, `coverage_class`, `object_source_kind`, the committed fixture source path when `object_source_kind = "original_sysroot_fixture"`, the harvested or built original object path relative to `safe/work/link-smoke/original-objects/**`, any required startfiles or archives, the exercised public DSOs or symbol families, and whether the relinked binary runs via the safe loader or directly.
- Phase 06 seeds that file with the startup-object, ordinary dynamic, PIE, static, static-PIE, and initial `GLIBC_PRIVATE` cases; later phases extend it in place instead of procedurally rescanning `safe/work/original-build/**`.
- Rewrite `safe/xtask/src/commands/link_compat_smoke.rs` from a one-step source-and-link smoke test into a two-stage original-object relink verifier.
- Stop compiling the compatibility cases directly against `work/install-root`.
- Add an explicit original-object stage that uses `safe/work/original-build/testroot.pristine` as the original sysroot and writes preserved `.o` artifacts under a dedicated scratch root such as `safe/work/link-smoke/original-objects/`.
- Prefer harvesting real upstream-built test objects from `safe/work/original-build/**` where they exist as stable standalone `.o` files; for gaps in that committed corpus, compile the fixed fixture sources named in `safe/generated/baseline/link-compat-corpus.json` once against the original sysroot and then treat those `.o` files as the previously compiled original-libc inputs for the relink check.
- Case discovery must come from `safe/generated/baseline/link-compat-corpus.json`, not from ad hoc directory scans during verification.
- The relink stage must consume those preserved `.o` files without recompiling them, link them against `work/install-root`, and run the resulting binaries through the safe loader or directly for static cases.
- Keep the existing coverage classes, but express them in terms of original-built objects: startup-object linkage beginning with `crt1.o`/`crti.o`/`crtn.o` plus an original-built `main.o`, and expanding by phase 10 to every shipped startfile variant `Mcrt1.o`, `Scrt1.o`, `crt1.o`, `gcrt1.o`, `grcrt1.o`, `rcrt1.o`, `crti.o`, and `crtn.o`, ordinary dynamic linkage, PIE, static linkage, static-PIE linkage, and at least one `GLIBC_PRIVATE` reference case.
- Port the phase-owned libc-family exports into Rust under `safe/crates/libc6/src/**` and `safe/crates/core-runtime/src/**`, confining `unsafe` to the ABI boundary, direct syscalls, TLS or stack interop, and generated forwarding veneers.
- Move string, memory, path, time, and stdio logic into safe Rust where possible, and add targeted unit tests for parser and edge-case behavior inside the Rust crates.
- Update the test harness so `run-original-tests` uses the Rust-backed install root for the ported phase-06 surfaces instead of the baseline public DSOs.
- Update `safe/upstream-compat/cve-status.toml` for `getrandom / arc4random`, `getrandom on powerpc` as either mitigated generically or `not-applicable` for amd64-only shipped code with rationale, `realpath`, `makecontext / unwinder interop`, `strftime`, and `PTR_MANGLE / pointer guard`.

## Verification Phases
### `check_06_phase_metadata_backcompat`
- `phase_id`: `check_06_phase_metadata_backcompat`
- `type`: `check`
- `bounce_target`: `impl_06_io_stdio_string_path`
- `purpose`: Prove the phase-metadata refactor did not break the landed phase-04 and phase-05 verifier commands or rename their CLI entrypoints.
- `commands`:
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

### `check_06_io_stdio_tests`
- `phase_id`: `check_06_io_stdio_tests`
- `type`: `check`
- `bounce_target`: `impl_06_io_stdio_string_path`
- `purpose`: Validate the phase-owned copied test tree and run the phase-owned upstream tests against the Rust-backed install root.
- `commands`:
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

### `check_06_io_stdio_abi`
- `phase_id`: `check_06_io_stdio_abi`
- `type`: `check`
- `bounce_target`: `impl_06_io_stdio_string_path`
- `purpose`: Verify ABI, SONAME, versioned exports, original-object relink compatibility, and headers for the libc-family surfaces that phase 06 first moves off baseline payloads.
- `commands`:
```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check-abi --dso libc --dso libpthread --dso libthread_db --dso libc_malloc_debug --dso libmemusage
cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
```

### `check_06_io_stdio_packages`
- `phase_id`: `check_06_io_stdio_packages`
- `type`: `check`
- `bounce_target`: `impl_06_io_stdio_string_path`
- `purpose`: Ensure the `.deb` packages now stage Rust-built public libc-family DSOs, prove installed-payload provenance for the first libc-family cutover, and rerun the already-shipped loader and runtime tool behaviors after that cutover.
- `commands`:
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

### `check_06_io_stdio_safety`
- `phase_id`: `check_06_io_stdio_safety`
- `type`: `check`
- `bounce_target`: `impl_06_io_stdio_string_path`
- `purpose`: Enforce the stronger safety mode once libc-family exported symbols start running through Rust code or generated forwarding veneers.
- `commands`:
```bash
cd safe
cargo run -p xtask -- audit-safety \
  --deny-unreviewed-unsafe \
  --deny-untracked-fallback-c \
  --require-cve-disposition
```

## Success Criteria
- `check_06_phase_metadata_backcompat` passes and the legacy phase-04 and phase-05 verifier commands remain available under their existing CLI names.
- `stage-upstream-build` is idempotent, matches the committed phase-05a staging semantics, and every checker that consumes `work/original-build` invokes it explicitly before doing so.
- The public `libc6` package payload and the phase-06-owned public `libc6-dev` `.so` link-name entries no longer point at `build/testroot.pristine/lib64/libc.so.6` or the equivalent baseline public DSO paths for `/usr/lib64/ld-linux-x86-64.so.2`, `/usr/lib64/libpthread.so.0`, `/usr/lib64/libthread_db.so.1`, `/usr/lib64/libc_malloc_debug.so.0`, and `/usr/lib64/libmemusage.so`, and `libc-family-cutover` proves those installed DSOs now come from Rust-built sources.
- Any still-shipped private backend DSO after phase 06 is explicitly listed as `private_baseline_backend_dso` under `/usr/libexec/safelibs/backends/**` in the manifests and ledgers.
- `check-abi`, the upgraded original-object `link-compat-smoke`, and `check-headers` pass against the Rust-backed install root, and `link-compat-smoke` is driven by the committed `safe/generated/baseline/link-compat-corpus.json` oracle rather than by ad hoc source recompilation or directory scanning.
- `check-owned-tests --owner-phase impl_06_io_stdio_string_path` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset passes under the Rust-backed install root.
- Installed `ld.so`, `ldd`, `ldconfig`, and `pldd` continue to pass their package-install smokes after the libc-family cutover.
- Any still-temporary `libc6-dev` startup, static, or audit assets remain explicitly inventoried in `safe/upstream-compat/package-scope.toml` as phase-10 obligations.
- `audit-safety` passes at the stronger phase-06 mode with explicit reviewed unsafe and fallback entries.

## Git Commit Requirement
Commit all phase-scoped changes to git before yielding. The commit must include the xtask refactor, the staged-upstream-build path, the relink oracle, the package and install manifest updates, the phase-owned test and ledger updates, and any required Rust implementation or veneer sources.
