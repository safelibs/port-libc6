# Phase Name
Final Backend Removal, Package Cutover, Top-Level Dependent Harness, and Full Safety Closure

## Implement Phase ID
`impl_10_final_fixup_and_audit`

## Preexisting Inputs
- All outputs from `impl_09_math_and_aux_dsos` and its passing checks.
- Existing authoritative inputs that must now be fully consumed by the final package and harness:
  - `dependents.json`
  - `relevant_cves.json`
  - `test-original.sh`
  - `safe/scripts/build-debs.sh`
  - `safe/scripts/install-safe-repo.sh`
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`
- Existing authoritative build and full-corpus test inputs that final closure must consume in place rather than recreate:
  - `original/**`
  - `safe/scripts/stage-original-build.sh`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/tests/manifest.toml`
- Existing shared harness inputs already materialized on disk and still authoritative, especially `safe/tests/support/**`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets under `safe/tests/scripts/**`.
- Existing zero-entry sentinels that must remain tracked through final closure:
  - `safe/tests/nscd/.gitkeep`
  - `safe/tests/po/.gitkeep`
  - `safe/tests/manual/.gitkeep`
- Preserve the consume-existing-artifacts contract: close out the committed authorities in place, keep `original/**` read-only, and use the staged upstream build helper instead of assuming `safe/work/original-build/**` already exists.

## New Outputs
- Final public package payloads with no shipped temporary fallback binaries, no private baseline backend DSOs, and no code-bearing `libc6-dev` startup, static, or development-link asset still sourced from `build_testroot`.
- Updated root-level dependent harness that installs the safe local apt repository before installing and smoke-testing runtime dependents and source-build dependents.
- Final CVE, package-scope, fallback, and safety ledgers with no phase-10 violations.

## File Changes
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
- Final tool and package metadata:
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

## Implementation Details
- Remove the temporary backend-forwarding mechanism from the shipped package payload. The build must now fail if any shipped public DSO would still need a baseline backend export.
- Private backend DSOs used during phases 06-09 must disappear from the package manifests, install manifests, package-scope ledger, fallback inventory, and installed package filesystem.
- Extend `safe/xtask/src/commands/audit_safety.rs` with a dedicated final flag such as `--deny-shipped-private-backend-dsos`, and update `safe/upstream-compat/safety-policy.toml` so the strongest mode and a fixed policy gate enforce the same cutoff.
- Finish the `libc6-dev` code-bearing link-asset cutover.
- `/usr/lib64/{Mcrt1.o,Scrt1.o,crt1.o,crti.o,crtn.o,gcrt1.o,grcrt1.o,rcrt1.o}` may remain assembly-backed on amd64, but they must come from checked-in safe-owned sources such as `safe/crates/compat-asm/x86_64/**` with final manifest provenance `compat_asm`.
- Replace `/usr/lib64/{libBrokenLocale.a,libanl.a,libc.a,libc_nonshared.a,libdl.a,libg.a,libm-2.39.a,libm.a,libmcheck.a,libmvec.a,libpthread.a,libpthread_nonshared.a,libresolv.a,librt.a,libutil.a}` with safe-owned archives or linker scripts whose members come from Rust-built objects plus the minimum documented asm shims.
- If `libpthread_nonshared.a` remains empty on amd64, mark it `synthetic_empty_archive` and explain the Debian compatibility requirement in `safe/upstream-compat/package-scope.toml`.
- Code-bearing `libc6-dev` DSOs and link names such as `/usr/lib64/libc.so`, `/usr/lib64/libm.so`, `/usr/lib64/libBrokenLocale.so`, `/usr/lib64/libanl.so`, `/usr/lib64/libc_malloc_debug.so`, `/usr/lib64/libmvec.so`, `/usr/lib64/libnss_compat.so`, `/usr/lib64/libnss_hesiod.so`, `/usr/lib64/libresolv.so`, `/usr/lib64/libthread_db.so`, and `/usr/lib64/audit/sotruss-lib.so` must resolve to safe-built artifacts or symlinks to safe-built DSOs. None may keep `source_origin = "build_testroot"`.
- Any final `build_testroot` entries still permitted in `safe/generated/baseline/package-files/libc6-dev.json` must be non-code headers, data-only files, documentation, or debug helpers with explicit `asset_kind` and safety-policy justification.
- Update `test-original.sh` so it actually tests the safe port. Ensure `safe/scripts/build-debs.sh` validates or materializes `safe/work/original-build/**`, builds a current release artifact set, and then packages it before the dependent harness runs.
- Keep the Docker-based dependent smoke model, preserve the current 16 runtime-dependent smoke checks and 3 source-build checks for `strace`, `valgrind`, and `libvirt`, and keep the source-repository setup inside Ubuntu 24.04.
- Make `safe/scripts/install-safe-repo.sh`, `safe/xtask/src/commands/test_package_install.rs`, and `test-original.sh` consume the same package version source as `safe/generated/packaging/package-build-manifest.json`.
- Extend `safe/xtask/src/commands/test_package_install.rs` with a final `dev-link-artifacts` smoke set that rejects final code-bearing `libc6-dev` assets with `build_testroot` provenance, byte-compares installed `.o`, `.a`, `.so`, and audit-helper payloads against safe-owned sources, and compiles and runs dynamic, PIE, static, and static-PIE samples that exercise every shipped startfile variant.
- Extend `safe/xtask/src/commands/test_package_install.rs` with a final `backend-payload-closure` smoke set that fails if any shipped entry still uses `asset_kind = "private_baseline_backend_dso"`, `classification = "private_baseline_backend_dso"`, or a shipped path under `/usr/libexec/safelibs/backends/**`.
- Finalize the upgraded link-compat verifier so it fails if any relinked original-built object still resolves through a temporary backend payload, requires recompilation against `work/install-root`, or uses a final `libc6-dev` startfile or static archive that still traces to `build_testroot`.
- Clean up package metadata so the final package set remains internally consistent and still limited to the required seven-package surface unless a checker proves expansion is necessary.
- Close every remaining open CVE row or mark it `not-applicable` with a concrete package-scope rationale.

## Verification Phases
### `check_10_workspace_unit_tests`
- `phase_id`: `check_10_workspace_unit_tests`
- `type`: `check`
- `bounce_target`: `impl_10_final_fixup_and_audit`
- `purpose`: Run the Rust workspace unit and integration tests that sit outside the copied upstream test corpus and package-install smokes.
- `commands`:
```bash
cd safe
cargo test --workspace
```

### `check_10_full_abi_link`
- `phase_id`: `check_10_full_abi_link`
- `type`: `check`
- `bounce_target`: `impl_10_final_fixup_and_audit`
- `purpose`: Run full release build, all-DSO ABI verification, header verification, final `libc6-dev` provenance auditing through the relink matrix, and original-object relink verification against the final package payload.
- `commands`:
```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check-abi --all-dsos
cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
```

### `check_10_full_upstream_tests`
- `phase_id`: `check_10_full_upstream_tests`
- `type`: `check`
- `bounce_target`: `impl_10_final_fixup_and_audit`
- `purpose`: Run the full copied upstream test corpus against the final Rust-backed install root.
- `commands`:
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

### `check_10_full_packages`
- `phase_id`: `check_10_full_packages`
- `type`: `check`
- `bounce_target`: `impl_10_final_fixup_and_audit`
- `purpose`: Verify the final `.deb` package set through the full smoke matrix, including the phase-06 public DSO provenance cutover check, explicit rejection of shipped private backend DSOs, and the final `libc6-dev` startup, static, and development-link provenance cutover.
- `commands`:
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

### `check_10_dependents_dropin`
- `phase_id`: `check_10_dependents_dropin`
- `type`: `check`
- `bounce_target`: `impl_10_final_fixup_and_audit`
- `purpose`: Verify the actual drop-in replacement goal by installing the safe packages and running the root-level dependent harness.
- `commands`:
```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cd ..
./safe/scripts/build-debs.sh
./test-original.sh
```

### `check_10_final_safety`
- `phase_id`: `check_10_final_safety`
- `type`: `check`
- `bounce_target`: `impl_10_final_fixup_and_audit`
- `purpose`: Enforce the phase-10 strongest safety mode, including zero shipped temporary fallback binaries and zero shipped private baseline backend DSOs.
- `commands`:
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

## Success Criteria
- `cargo test --workspace` passes.
- `check-abi --all-dsos` passes.
- `check-owned-tests --all-ported` proves that all 5,584 catalog rows are marked `ported`, the zero-entry sentinels are present, and the executable corpus passes.
- Every `.deb` smoke bucket passes, including `backend-payload-closure` and `dev-link-artifacts`.
- `backend-payload-closure` proves the manifests, ledgers, and installed package filesystem no longer ship any `private_baseline_backend_dso` payload.
- The updated root-level `./test-original.sh` passes end to end.
- `audit-safety` passes with both `--deny-shipped-temporary-fallback-binaries` and `--deny-shipped-private-backend-dsos`.
- `link-compat-smoke` passes by relinking the cumulative committed cases from `safe/generated/baseline/link-compat-corpus.json`, not by recompiling them against the safe headers or DSOs, and it fails if any final `libc6-dev` startfile or static archive still comes from `build_testroot`.
- No shipped package or install manifest row, package-scope entry, fallback-inventory entry, or installed filesystem path remains under `/usr/libexec/safelibs/backends/**`.
- The final `libc6-dev` manifest contains no code-bearing `build_testroot` entries; any remaining `build_testroot` rows are non-code and explicitly justified in `safe/upstream-compat/package-scope.toml` and `safe/upstream-compat/safety-policy.toml`.

## Git Commit Requirement
Commit all phase-scoped changes to git before yielding. The commit must include the backend-removal enforcement, final `libc6-dev` provenance cutover, root-level dependent harness updates, final manifest and ledger cleanup, and every required package-install and safety-policy change.
