# Final Backend Audit

**Phase Name**

Final backend removal, package cutover, top-level dependent harness, and full safety closure

**Implement Phase ID**

`impl_10_final_fixup_and_audit`

**Preexisting Inputs**

- All outputs from `impl_09_math_and_aux_dsos` and its passing checks.
- Cumulative phase-06 through phase-09 artifacts:
  - all public DSO and helper-tool cutovers
  - `safe/tests/nscd/.gitkeep`
  - `safe/tests/po/.gitkeep`
  - `safe/tests/manual/.gitkeep`
  - `safe/tests/manifest.toml`, expected to have all 5,584 catalog rows marked `ported` after phase 09
  - `safe/generated/baseline/link-compat-corpus.json` with cumulative original-object relink cases
  - cumulative package smoke support through `dev-and-time-tools`
- Root-level and package-harness authorities:
  - `dependents.json`
  - `test-original.sh`
  - `safe/scripts/build-debs.sh`
  - `safe/scripts/install-safe-repo.sh`
  - `safe/scripts/stage-original-build.sh`
  - `safe/generated/packaging/package-build-manifest.json`
- Final package, install, test, and safety authorities:
  - `original/**`
  - `safe/generated/security/relevant-cves-index.json`
  - `relevant_cves.json`
  - `safe/generated/baseline/abi/*.json`
  - `safe/generated/version-scripts/*.map`
  - `safe/generated/baseline/package-files/*.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
- Final development-link asset authorities and sources:
  - `safe/generated/baseline/package-files/libc6-dev.json`
  - `safe/debian/libc-dev.install`
  - `safe/debian/libc-dev.lintian-overrides`
  - `safe/crates/compat-asm/x86_64/**`
  - Rust-built objects, archives, linker scripts, and symlink targets produced by the phase-09 workspace build
- If already present, the ignored derived `safe/work/original-build/**` tree. Otherwise final checkers must run `stage-upstream-build` before consuming it; it is not a required preexisting workflow input.

**New Outputs**

- Final public package payloads with no shipped temporary fallback binaries, no shipped private baseline backend DSOs, and no code-bearing `libc6-dev` startup/static/dev-link/audit asset still sourced from `build_testroot`.
- Final `libc6-dev` provenance for every code-bearing `/usr/lib*/**` entry:
  - startup objects `/usr/lib64/{Mcrt1.o,Scrt1.o,crt1.o,crti.o,crtn.o,gcrt1.o,grcrt1.o,rcrt1.o}` emitted from safe-owned sources such as `safe/crates/compat-asm/x86_64/**` with final provenance `compat_asm`
  - static archives `/usr/lib64/{libBrokenLocale.a,libanl.a,libc.a,libc_nonshared.a,libdl.a,libg.a,libm-2.39.a,libm.a,libmcheck.a,libmvec.a,libpthread.a,libpthread_nonshared.a,libresolv.a,librt.a,libutil.a}` replaced with safe-owned archives, linker scripts, or documented compatibility shells
  - public DSO link names and audit helper paths `/usr/lib64/{libc.so,libm.so,libBrokenLocale.so,libanl.so,libc_malloc_debug.so,libmvec.so,libnss_compat.so,libnss_hesiod.so,libresolv.so,libthread_db.so}` and `/usr/lib64/audit/sotruss-lib.so` resolving to safe-built artifacts or symlinks to safe-built DSOs
  - `libpthread_nonshared.a` recorded as `synthetic_empty_archive` if it remains an empty amd64 compatibility archive
- Extended package-install smokes for `backend-payload-closure` and `dev-link-artifacts`.
- Updated root-level dependent harness that installs the safe local apt repository before running runtime-dependent and source-build dependent checks.
- Final CVE, package-scope, fallback, port-status, and safety ledgers with no phase-10 violations.

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

- Remove temporary backend forwarding from the shipped package payload. The build must fail if any shipped public DSO would still need a baseline backend export.
- Private backend DSOs used during phases 06-09 must disappear from package manifests, install manifests, package-scope ledger, fallback inventory, and installed package filesystem.
- Extend `audit_safety.rs` with a final flag such as `--deny-shipped-private-backend-dsos`. It must fail if any shipped `safe/upstream-compat/package-scope.toml` entry still has `asset_kind = "private_baseline_backend_dso"` or any shipped `safe/generated/baseline/fallback-c-inventory.json` entry still has `classification = "private_baseline_backend_dso"`.
- Update `safe/upstream-compat/safety-policy.toml` so the phase-10 strongest mode includes `--deny-shipped-private-backend-dsos`, and add a fixed policy gate such as `deny_shipped_private_backend_dso_by_phase = "impl_10_final_fixup_and_audit"`.
- Finish the `libc6-dev` code-bearing link-asset cutover. Startup objects `/usr/lib64/{Mcrt1.o,Scrt1.o,crt1.o,crti.o,crtn.o,gcrt1.o,grcrt1.o,rcrt1.o}` may remain assembly-backed on amd64, but must be emitted from checked-in safe-owned sources with final manifest provenance `compat_asm`; none may be copied from `build/testroot.pristine`.
- Replace `/usr/lib64/{libBrokenLocale.a,libanl.a,libc.a,libc_nonshared.a,libdl.a,libg.a,libm-2.39.a,libm.a,libmcheck.a,libmvec.a,libpthread.a,libpthread_nonshared.a,libresolv.a,librt.a,libutil.a}` with safe-owned archives or linker scripts whose members come from Rust-built objects plus minimum documented asm shims.
- `libpthread_nonshared.a` may remain an empty amd64 compatibility archive only if the manifest marks it `synthetic_empty_archive` and `safe/upstream-compat/package-scope.toml` explains the Debian compatibility requirement.
- If `libg.a`, `libmcheck.a`, or another archive remains a thin compatibility shell rather than a full Rust archive, record the exact rationale in `safe/upstream-compat/package-scope.toml` and `safe/upstream-compat/safety-policy.toml`.
- Code-bearing `libc6-dev` DSOs and link names such as `/usr/lib64/libc.so`, `/usr/lib64/libm.so`, `/usr/lib64/libBrokenLocale.so`, `/usr/lib64/libanl.so`, `/usr/lib64/libc_malloc_debug.so`, `/usr/lib64/libmvec.so`, `/usr/lib64/libnss_compat.so`, `/usr/lib64/libnss_hesiod.so`, `/usr/lib64/libresolv.so`, `/usr/lib64/libthread_db.so`, and `/usr/lib64/audit/sotruss-lib.so` must resolve to safe-built artifacts or symlinks to safe-built DSOs.
- The only final `build_testroot` entries still permitted in `libc6-dev.json` are non-code headers, generated-but-data-only files, documentation, or debug helpers whose `asset_kind`, package-scope entry, and safety-policy entry mark them non-executable and explain why copying is acceptable.
- Update `test-original.sh` so it tests the safe port, not stock Ubuntu libc6. Keep the Docker-based dependent smoke model, preserve the current 16 runtime-dependent smoke checks and 3 source-build checks for `strace`, `valgrind`, and `libvirt`, and keep Ubuntu 24.04 source-repository setup for source builds.
- `safe/scripts/build-debs.sh` must explicitly run `cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build` and then `cargo run -p xtask -- build --target amd64 --profile release` before `package-deb`, unless `package-deb` itself enforces both prerequisites internally.
- Inside the dependent-harness container, call `safe/scripts/install-safe-repo.sh` before installing the seven required packages: `libc6`, `libc6-dev`, `libc6-dbg`, `libc-bin`, `libc-dev-bin`, `locales`, and `nscd`.
- Make `safe/scripts/install-safe-repo.sh`, `test_package_install.rs`, `test-original.sh`, and `package_deb.rs` derive or receive the expected package version from `safe/generated/packaging/package-build-manifest.json` or the same shared helper.
- Add the final `dev-link-artifacts` smoke set. It must inspect `libc6-dev.json`, `required-packages.json`, and `package-scope.toml`, fail if any final code-bearing `libc6-dev` asset still uses `build_testroot` provenance, and byte-compare installed `.o`, `.a`, `.so`, and audit-helper payloads against safe-owned `source_path` entries recorded in manifests.
- `dev-link-artifacts` must compile and run at least one dynamic, PIE, static, and static-PIE sample against installed `libc6-dev`, and must explicitly exercise every shipped startfile variant: `Mcrt1.o`, `Scrt1.o`, `crt1.o`, `gcrt1.o`, `grcrt1.o`, `rcrt1.o`, `crti.o`, and `crtn.o`, either through compiler-driver flags or direct linker invocations.
- Add the final `backend-payload-closure` smoke set. It must inspect package manifests, install manifests, `package-scope.toml`, and `fallback-c-inventory.json`; fail if any shipped entry still uses `asset_kind = "private_baseline_backend_dso"`, `classification = "private_baseline_backend_dso"`, or a shipped path under `/usr/libexec/safelibs/backends/**`; and fail inside the installed container if any backend DSO remains installed or byte-matches a private-backend manifest entry.
- Finalize `link_compat_smoke.rs` for the no-backend end state. It must fail if any relinked original-built object resolves through a temporary backend payload, requires recompilation against `work/install-root`, or uses a final `libc6-dev` startfile or static archive that traces to `build_testroot`.
- The final smoke corpus must be the committed cumulative `safe/generated/baseline/link-compat-corpus.json`. By the end of phase 10 it must cover dynamic, PIE, static, static-PIE, startup-object, profiling-startfile, and `GLIBC_PRIVATE` cases using preserved original-built `.o` inputs, and should include representative original test objects from the staged upstream build where available.
- Keep the final package set internally consistent and limited to the required seven-package surface unless a checker proves expansion is necessary.
- Close every remaining CVE row or mark it `not-applicable` with concrete package-scope rationale.
- Ensure the final safety policy's strongest mode passes without exceptions.

**Verification Phases**

- `check_10_workspace_unit_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_10_final_fixup_and_audit`
  - Purpose: Run Rust workspace unit and integration tests outside the copied upstream corpus and package-install smokes.
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
  - Purpose: Verify final `.deb` packages through the full smoke matrix, including phase-06 public DSO provenance, explicit rejection of shipped private backend DSOs, and final `libc6-dev` startup/static/dev-link provenance.
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
  - Purpose: Verify the drop-in replacement goal by installing the safe packages and running the root-level dependent harness.
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
  - Purpose: Enforce the strongest safety mode, including zero shipped temporary fallback binaries and zero shipped private baseline backend DSOs.
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

**Success Criteria**

- All six `check_10_*` phases pass in order.
- `cargo test --workspace`, `check-abi --all-dsos`, header verification, and final original-object relink verification pass.
- `check-owned-tests --all-ported` proves all 5,584 catalog rows are `ported`, zero-entry sentinels are present and tracked, and the executable corpus passes.
- Every package smoke bucket passes, including `backend-payload-closure` and `dev-link-artifacts`.
- `./test-original.sh` passes end-to-end using the safe package set.
- `audit-safety` passes with `--deny-shipped-temporary-fallback-binaries` and `--deny-shipped-private-backend-dsos`.
- `link-compat-smoke` relinks cumulative committed cases from `safe/generated/baseline/link-compat-corpus.json`, not sources recompiled against safe headers or DSOs, and fails if any final `libc6-dev` startfile or static archive comes from `build_testroot`.
- No shipped package manifest row, install manifest row, package-scope entry, fallback-inventory entry, or installed filesystem path remains under `/usr/libexec/safelibs/backends/**`.
- The final `libc6-dev` manifest contains no code-bearing `build_testroot` entries; any remaining `build_testroot` rows are non-code and explicitly justified in package-scope and safety-policy ledgers.
- The package surface remains the seven required packages (`libc6`, `libc6-dev`, `libc6-dbg`, `libc-bin`, `libc-dev-bin`, `locales`, `nscd`) unless a checker proves expansion is necessary.

**Git Commit Requirement**

The implementer must commit all phase-owned work to git before yielding.
