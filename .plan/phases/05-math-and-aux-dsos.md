# Phase Name
Math, Wide-Char, Dlfcn/Rt/Util/Debug/Sunrpc, and Remaining Helper-Tool Cutover

## Implement Phase ID
`impl_09_math_and_aux_dsos`

## Preexisting Inputs
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
- Existing authoritative build, package, and test inputs that phase 09 must extend in place rather than rediscover:
  - `original/**`
  - `safe/scripts/stage-original-build.sh`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
- Existing shared harness inputs already materialized on disk and still authoritative, especially `safe/tests/support/**`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets under `safe/tests/scripts/**`.
- Existing copied-test roots already materialized before phase 09 and touched again here must be updated in place rather than rematerialized:
  - `safe/tests/sysdeps/**`
  - `safe/tests/sysdeps-x86_64/**`
- Preserve the consume-existing-artifacts contract: extend committed manifests, relink cases, tests, and ledgers in place and keep the normalized `sysdeps*` destinations and zero-entry assignments fixed.

## New Outputs
- Rust-backed public implementations for the remaining auxiliary DSOs and the math-heavy libc exports.
- First-class shipped implementations for:
  - `/usr/bin/gencat`
  - `/usr/bin/getconf`
  - `/usr/bin/tzselect`
  - `/usr/bin/zdump`
  - `/usr/sbin/zic`
- Ported tests for the remaining 2,088 planned entries.
- Explicit coverage of remaining phase-owned special assets, including `catgets` message catalogs, the lone `gnulib` test, the shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` manifest rows, and the zero-entry sentinel `safe/tests/manual/.gitkeep`.
- Near-final package manifests and install manifests with only temporary backend DSOs and the explicitly inventoried final `libc6-dev` startup, static, and audit cutover work left as phase-10 cleanup.

## File Changes
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
  - `safe/tests/manual/.gitkeep`
  - `safe/tests/scripts/check-wrapper-headers.py`
  - `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/safety-policy.toml`

## Implementation Details
- Port `libm` and `libmvec` without breaking ABI or performance-sensitive calling conventions. Prefer safe Rust for scalar math logic where feasible, and isolate unavoidable assembly or intrinsic shims in explicit reviewed `asm_shim` entries.
- Whenever this phase changes a shipped helper binary, dev or time tool path, auxiliary DSO payload, or other package-installed asset, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits.
- Expand the upgraded original-object relink verifier so the phase-09 smoke corpus also covers math and auxiliary DSO references. Extend `safe/generated/baseline/link-compat-corpus.json` in place with original-built object cases that require `libm`, `libdl`, `librt`, and `libutil` symbols, and keep them as preserved `.o` inputs under the relink workflow.
- Port `dlopen`, `dlsym`, `dlvsym`, and `dladdr`-related public surfaces consistently with the existing Rust loader control-plane logic from phase 04.
- Replace the remaining helper-tool wrappers owned by phase 09.
- Port the wide-character, message-catalog, timezone, sunrpc, login, and auxiliary runtime surfaces and their tests.
- Materialize the large set of `source_path == null` phase-09 assets through the generalized placeholder or scaffold path introduced earlier, especially the `catgets` `.cat` fixtures, header-check placeholders, gmon compare files, and other generated test artifacts.
- Explicitly port the `catgets` and `gnulib` subtrees named in `safe/generated/baseline/test-port-plan.json`, keep the normalized `safe/tests/sysdeps/**` and `safe/tests/sysdeps-x86_64/**` destinations aligned with the manifest, and carry `safe/tests/manual/.gitkeep` forward as the owned zero-entry sentinel for a destination with no executable tests.
- Shared script files remain single checked-in assets; mark only the catalog rows that point at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py` as `ported` in `safe/tests/manifest.toml`.
- Update `safe/upstream-compat/cve-status.toml` for the remaining rows owned here, including `memcmp x32`, `sunrpc svc_run`, and any still-open regex, glob, or resource rows that actually land in this phase’s owned surface.

## Verification Phases
### `check_09_math_tests`
- `phase_id`: `check_09_math_tests`
- `type`: `check`
- `bounce_target`: `impl_09_math_and_aux_dsos`
- `purpose`: Run the large remaining test corpus for math and auxiliary DSOs.
- `commands`:
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

### `check_09_math_abi`
- `phase_id`: `check_09_math_abi`
- `type`: `check`
- `bounce_target`: `impl_09_math_and_aux_dsos`
- `purpose`: Verify all remaining auxiliary DSOs, their versioned exports, and original-object relink compatibility.
- `commands`:
```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check-abi --dso libdl --dso libm --dso libmvec --dso libpcprofile --dso librt --dso libutil --dso libc
cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
```

### `check_09_math_packages`
- `phase_id`: `check_09_math_packages`
- `type`: `check`
- `bounce_target`: `impl_09_math_and_aux_dsos`
- `purpose`: Verify cumulative installed-package coverage after the remaining DSO and helper-tool cutovers, including required-package and debug coherence, prior installed surfaces, and the new dev and time tools.
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
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set network-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set locale-tools
cargo run -p xtask -- test-package-install --deb-dir work/debs --smoke-set dev-and-time-tools
```

### `check_09_math_safety`
- `phase_id`: `check_09_math_safety`
- `type`: `check`
- `bounce_target`: `impl_09_math_and_aux_dsos`
- `purpose`: Enforce safety and CVE disposition for the remaining auxiliary runtime surfaces.
- `commands`:
```bash
cd safe
cargo run -p xtask -- audit-safety \
  --deny-unreviewed-unsafe \
  --deny-untracked-fallback-c \
  --require-cve-disposition
```

## Success Criteria
- All phase-09 helper tools are no longer temporary fallback wrappers.
- All remaining non-final DSOs pass `check-abi`.
- The phase-09 relink smoke still proves that the phase-06 through phase-09 entries committed in `safe/generated/baseline/link-compat-corpus.json` link and run against the safe install root.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, `locale-tools`, and `dev-and-time-tools`, so required-package and debug coherence plus every earlier installed surface stay live after the final phase-09 DSO and tool cutovers.
- `check-owned-tests --owner-phase impl_09_math_and_aux_dsos` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset, including `catgets`, `gnulib`, `mathvec`, the shared script rows, and the normalized `sysdeps` destinations, passes under the Rust-backed install root.
- The tracked `safe/tests/manual/.gitkeep` sentinel remains explicitly covered even though that destination has no executable tests.
- `audit-safety` passes with reviewed unsafe and fallback coverage plus current CVE dispositions.

## Git Commit Requirement
Commit all phase-scoped changes to git before yielding. The commit must include the remaining DSO and helper-tool cutovers, the manifest and ledger updates, the relink corpus extensions, the phase-owned tests, and the tracked `safe/tests/manual/.gitkeep` sentinel.
