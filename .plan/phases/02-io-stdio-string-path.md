# IO Stdio String Path

**Phase Name**

I/O, stdio, string, path, time, and first real libc-family cutover

**Implement Phase ID**

`impl_06_io_stdio_string_path`

**Preexisting Inputs**

- All outputs from `impl_05a_commit_prepared_safe_frontier`, including the committed phase-02 through phase-05 workspace state.
- Phase-05a committed authorities and helpers:
  - `safe/xtask/src/common.rs`
  - `safe/xtask/src/commands/build.rs`
  - `safe/generated/baseline/committed-safe-frontier.txt`
  - `safe/scripts/stage-original-build.sh`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/*.toml`
- ABI and version-script authorities:
  - `safe/generated/baseline/abi/libc.json`
  - `safe/generated/baseline/abi/libpthread.json`
  - `safe/generated/baseline/abi/libthread_db.json`
  - `safe/generated/baseline/abi/libc_malloc_debug.json`
  - `safe/generated/baseline/abi/libmemusage.json`
  - `safe/generated/version-scripts/libc.map`
  - `safe/generated/version-scripts/*.map`
- Package and install authorities:
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc6-dev.json`
  - `safe/generated/baseline/package-files/libc6-dbg.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/generated/packaging/package-build-manifest.json`
- Test authorities and shared harness inputs:
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/tests/manifest.toml`
  - `safe/tests/support/**`
  - `safe/tests/support/glibcpp.py`
  - `safe/tests/include/**`
  - `safe/tests/bits/**`
  - `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`
  - `safe/tests/test-skeleton.c`
  - `safe/tests/c++-types.data`
  - `safe/tests/scripts/**`
  - existing normalized destinations under `safe/tests/libio/**`, `safe/tests/stdlib/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`
- Read-only external authorities: `original/**`, `dependents.json`, `relevant_cves.json`, and `test-original.sh`.
- If already present, the ignored derived `safe/work/original-build/**` tree. Otherwise the new `stage-upstream-build` command must validate or recreate it before checkers consume it; it is not a required preexisting workflow input.

**New Outputs**

- `cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build`, mirroring the committed `safe/scripts/stage-original-build.sh` semantics. It validates and adopts any already materialized `safe/work/original-build/**` tree or creates it without rewriting committed authorities.
- A committed relink oracle at `safe/generated/baseline/link-compat-corpus.json` that fixes original-object relink coverage for this and later phases.
- A real Rust-backed build path for libc-family DSOs using generated forwarding veneers instead of zero-return stubs.
- Updated package manifests, install manifests, package-scope entries, and installed-package cutover smokes that stage Rust-built public libc-family DSOs plus private baseline backend copies.
- Explicit inventory for any still-temporary `libc6-dev` startup, static, audit, or development-link artifacts that remain on `build_testroot` until phase 10.
- Ported test sources for the 654 phase-owned entries covering `stdio-common`, `stdlib`, `libio`, `string`, `io`, `time`, `dirent`, `assert`, `ctype`, `termios`, `timezone`, shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` rows, generated per-subdir `check-installed-headers-*` placeholders, and normalized `safe/tests/{sysdeps,sysdeps-x86_64,sysdeps-linux-x86_64}/**` destinations.
- Updated safety and CVE status ledgers for runtime, path, time, random, unwind, and pointer-guard issues that stop being baseline-backend exceptions.

**File Changes**

- Phase metadata and verifier orchestration:
  - `safe/xtask/src/common.rs`
  - `safe/xtask/src/main.rs`
  - `safe/xtask/src/commands/mod.rs`
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
- Build, install, package, link, and header cutover:
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
  - generated or checked-in veneer sources under `safe/crates/compat-asm/x86_64/**`
- Test tree and ledgers:
  - `safe/tests/manifest.toml`
  - phase-owned files under `safe/tests/stdio-common/**`, `safe/tests/stdlib/**`, `safe/tests/libio/**`, `safe/tests/string/**`, `safe/tests/io/**`, `safe/tests/time/**`, `safe/tests/dirent/**`, `safe/tests/assert/**`, `safe/tests/ctype/**`, `safe/tests/termios/**`, `safe/tests/timezone/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`
  - shared phase-owned manifest rows for `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
  - `safe/README.md`

**Implementation Details**

- Replace the stub-only hybrid-shell generator in `safe/xtask/src/commands/build.rs` with a real incremental DSO builder.
- Keep the checked-in ABI JSON and generated version scripts as the authoritative export list. Generate x86_64 assembly veneers for every exported symbol still missing from the Rust object set for that DSO.
- Resolve forwarded symbols by exact version using `dlvsym` from privately staged baseline backend DSOs, not ordinary `dlsym`. Bind versioned public names with `.symver`.
- Fail the build if a Rust-provided symbol collides with a generated fallback veneer name or if a baseline export cannot be resolved in the private backend copy.
- Replace phase-05-specific frontier constants and refresh logic with explicit per-command expected owner IDs and a data-driven current frontier list.
- Preserve the legacy CLI command names `check_04_loader_tests`, `check_04_loader_abi`, `check_04_loader_tools`, `check_05_core_runtime_tests`, `check_05_core_runtime_abi`, `check_05_runtime_tools`, and `check_05_base_dependent_smoke`.
- Rewrite legacy command modules as thin wrappers over phase-aware helpers, each with fixed owner phase, DSO set, helper-path assertions, and install-root defaults.
- Preserve the current extra invariants in the phase-04 and phase-05 wrappers: normalized `sysdeps*` destination expectations for phase 04, and the phase-05 stdlib allowlist for `tst-arc4random-*` plus `tst-getrandom`.
- Stop hard-coding phase-05 prose into generated READMEs and manifest notes. Preserve already-committed later-phase `safe/tests/manifest.toml` status changes instead of regenerating post-phase-05 rows back to `planned`.
- Factor staged upstream-build preparation out of baseline ingestion. The new `stage-upstream-build` command must call `safe/scripts/stage-original-build.sh` or exactly mirror its validation list, `configparms`, configure flags, and `make testroot.pristine/install.stamp` sequence. It must never rewrite checked-in generated baselines.
- From this phase onward, generated checkers run `stage-upstream-build` explicitly before the first consumer of `work/original-build`. Commands such as `check-owned-tests`, `link-compat-smoke`, `package-deb`, and preserved phase-04/05 wrappers may assume it has run, but must not silently restage the tree through ad hoc logic.
- Generalize `sync_safe_tests_tree()`, `generate_tests_manifest()`, and `write_generated_test_placeholder()` so later phases can materialize `source_path == null` entries. This is required for phase-06-owned `check-installed-headers-*`, `test-as-const-*`, and compare-output fixtures.
- Do not reassign shared top-level script rows such as `safe/tests/scripts/check-installed-headers.sh` or `safe/tests/scripts/check-local-headers.sh`; they remain preexisting support assets.
- Introduce `cargo run -p xtask -- check-owned-tests`. With `--owner-phase <phase-id>`, it must validate completeness against `safe/generated/baseline/test-port-plan.json`, `safe/generated/baseline/test-catalog.json`, and `safe/tests/manifest.toml`; every owned row must exist exactly once, be marked `ported`, have its committed `safe_path` present, and have all referenced support paths present.
- `check-owned-tests --owner-phase` must verify exact root materialization, normalized `sysdeps*` destinations, and any owned zero-entry destination sentinel. It must fail before running tests if any owned executable row is still `planned`, any owned file is missing, or any required root or sentinel is absent.
- The executable pass must select exact catalog rows by owner phase or equivalent exact entry selection. It must not infer ownership with broad `run-original-tests --subdirs ...` filters.
- With `--all-ported`, the checker must fail unless all 5,584 manifest entries are marked `ported`, every referenced path exists, and `safe/tests/nscd/.gitkeep`, `safe/tests/po/.gitkeep`, and `safe/tests/manual/.gitkeep` are present and tracked.
- Extend `install_root.rs` and `package_deb.rs` so package entries can stage Rust-built DSOs from the active build root and later safe-owned `libc6-dev` compatibility objects or archives.
- After this phase, `build/testroot.pristine` may remain only for private backend copies and explicitly inventoried temporary `libc6-dev` startup, static, and audit assets. No public runtime DSO or public `libc6-dev` link-name `.so` may still use `build_testroot`.
- Every remaining private backend DSO must be staged under `/usr/libexec/safelibs/backends/**` or an equivalent non-public prefix, recorded as `asset_kind = "private_baseline_backend_dso"` in package manifests, install manifests, and `safe/upstream-compat/package-scope.toml`, and recorded as `classification = "private_baseline_backend_dso"` in `safe/generated/baseline/fallback-c-inventory.json`.
- Continue synthesizing `libpthread_nonshared.a` and preserving `lib64` mirrors, but record that archive as a generated compatibility asset rather than leaving it implicit outside the manifests.
- Add the `libc-family-cutover` smoke set. It must inspect `libc6.json`, `libc6-dev.json`, and `required-packages.json` and fail unless `/usr/lib64/ld-linux-x86-64.so.2`, `/usr/lib64/libc.so.6`, `/usr/lib64/libpthread.so.0`, `/usr/lib64/libthread_db.so.1`, `/usr/lib64/libc_malloc_debug.so.0`, `/usr/lib64/libmemusage.so`, and corresponding phase-06-owned public `libc6-dev` `.so` link names no longer use `source_origin = "build_testroot"` or public `build/testroot.pristine/lib64/...` source paths.
- The installed-package smoke must compare installed public DSOs and phase-06-owned `libc6-dev` public `.so` link names against staged Rust-built source paths using ELF build IDs, hashes, or an equivalently strict provenance check. It must fail if a public phase-06 DSO still matches a baseline public copy.
- The same smoke must rerun installed `ld.so`, `ldd`, `ldconfig`, and `pldd` behaviors.
- Remove duplicated hard-coded safe package version literals. `package_deb.rs`, `test_package_install.rs`, and `safe/scripts/install-safe-repo.sh` must consume the version from `safe/generated/packaging/package-build-manifest.json` or a single shared helper.
- Add `safe/generated/baseline/link-compat-corpus.json`; `link-compat-smoke` must fail if this file is missing.
- Every relink case must record at least `case_id`, `owner_phase`, `coverage_class`, `object_source_kind`, fixture source path when `object_source_kind = "original_sysroot_fixture"`, preserved-object path relative to `safe/work/link-smoke/original-objects/**`, required startfiles or archives, exercised public DSOs or symbol families, and whether the relinked binary runs through the safe loader or directly.
- Phase 06 seeds the corpus with startup-object, ordinary dynamic, PIE, static, static-PIE, and initial `GLIBC_PRIVATE` cases. Later phases extend this one file in place.
- Rewrite `link_compat_smoke.rs` from source-and-link into a two-stage original-object relink verifier. The first stage must harvest real upstream-built objects or compile fixed fixture sources once against the original sysroot and preserve `.o` files. The relink stage must consume those `.o` files without recompiling them, link against `work/install-root`, and run binaries through the safe loader or directly for static cases.
- Coverage must be expressed in original-built object terms: startup-object linkage beginning with `crt1.o`/`crti.o`/`crtn.o` plus original-built `main.o`, ordinary dynamic linkage, PIE, static linkage, static-PIE linkage, and at least one `GLIBC_PRIVATE` reference case. Phase 10 expands startup coverage to every shipped variant.
- Port string, memory, path, time, and stdio logic into safe Rust where possible. Keep `unsafe` confined to C ABI boundaries, direct syscalls, TLS/stack/`setjmp` interop, and generated forwarding veneers.
- Update the test harness so `run-original-tests` uses the Rust-backed install root for ported phase-06 surfaces.
- Close or update CVE rows for `getrandom / arc4random`, `getrandom on powerpc` with amd64 package-scope rationale if applicable, `realpath`, `makecontext / unwinder interop`, `strftime`, and `PTR_MANGLE / pointer guard`.

**Verification Phases**

- `check_06_phase_metadata_backcompat`
  - Type: `check`
  - Fixed `bounce_target`: `impl_06_io_stdio_string_path`
  - Purpose: Prove the phase-metadata refactor did not break landed phase-04/05 verifier commands or rename their CLI entrypoints.
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
  - Purpose: Ensure the `.deb` packages now stage Rust-built public libc-family DSOs, prove installed-payload provenance for the first libc-family cutover, and rerun already-shipped loader/runtime tool behaviors after that cutover.
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

**Success Criteria**

- All five check phases pass in order.
- Legacy phase-04/05 verifier commands run successfully under their existing CLI names and preserve their extra invariants.
- `stage-upstream-build` is idempotent, matches phase-05a staging semantics, and every checker that consumes `work/original-build` invokes it explicitly before that consumption.
- Public `libc6` runtime DSOs and phase-06-owned public `libc6-dev` `.so` link names no longer point at baseline public DSO paths, and `libc-family-cutover` proves installed DSOs come from Rust-built sources.
- Private backend DSOs are explicitly listed as `private_baseline_backend_dso` under `/usr/libexec/safelibs/backends/**` in manifests and ledgers.
- `check-abi`, upgraded original-object `link-compat-smoke`, and `check-headers` pass against the Rust-backed install root.
- `link-compat-smoke` is driven by `safe/generated/baseline/link-compat-corpus.json`, not ad hoc source recompilation or directory scanning.
- `check-owned-tests --owner-phase impl_06_io_stdio_string_path` proves every phase-owned catalog row is materialized, marked `ported`, and passing where executable.
- Installed `ld.so`, `ldd`, `ldconfig`, and `pldd` package smokes still pass.
- Temporary `libc6-dev` startup, static, and audit assets are explicitly inventoried as phase-10 obligations.
- `audit-safety` passes with reviewed unsafe/fallback entries and required CVE disposition.

**Git Commit Requirement**

The implementer must commit all phase-owned work to git before yielding.
