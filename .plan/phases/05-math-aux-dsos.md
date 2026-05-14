# Math Aux DSOs

**Phase Name**

Math, wide-char, dlfcn/rt/util/debug/sunrpc, and remaining helper-tool cutover

**Implement Phase ID**

`impl_09_math_and_aux_dsos`

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
  - `safe/generated/baseline/abi/libc.json`
  - `safe/generated/version-scripts/*.map`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/libc-dev-bin.json`
  - `safe/generated/install-manifests/required-packages.json`
  - `safe/generated/install-manifests/test-install-root.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/package-scope.toml`
  - `safe/tests/manifest.toml`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
  - `relevant_cves.json`
- If already present, the ignored derived `safe/work/original-build/**` tree. Otherwise checkers must run `stage-upstream-build` before consuming it; it is not a required preexisting workflow input.

**New Outputs**

- Rust-backed public implementations for the remaining auxiliary DSOs and math-heavy libc exports.
- First-class shipped implementations for `/usr/bin/gencat`, `/usr/bin/getconf`, `/usr/bin/tzselect`, `/usr/bin/zdump`, and `/usr/sbin/zic`.
- Ported tests for the remaining 2,088 planned entries.
- Explicit coverage of phase-owned special assets, including `catgets` message catalogs, the lone `gnulib` test, shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` manifest rows, and the zero-entry sentinel `safe/tests/manual/.gitkeep`.
- Near-final package manifests and install manifests with only temporary backend DSOs and explicitly inventoried final `libc6-dev` startup/static/audit cutover work left as phase-10 cleanup.

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
  - activation of `safe/crates/aux-dsos/**` beyond the current placeholder as needed
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
  - shared phase-owned manifest rows for `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/safety-policy.toml`

**Implementation Details**

- Port `libm` and `libmvec` without breaking ABI or performance-sensitive calling conventions.
- Prefer safe Rust implementations for scalar math logic where feasible. For architecture-specific vector entrypoints, isolate unavoidable assembly or compiler-intrinsic shims in explicit `asm_shim` entries with narrow reviewed unsafe boundaries.
- Whenever this phase changes a shipped helper binary, dev/time tool path, auxiliary DSO payload, or other package-installed asset, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as corresponding `safe/generated/baseline/package-files/*.json` edits.
- Keep package-smoke coverage cumulative: `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, `locale-tools`, and new `dev-and-time-tools`.
- Expand original-object relink coverage by extending `safe/generated/baseline/link-compat-corpus.json` in place with cases requiring `libm`, `libdl`, `librt`, and `libutil` symbols. Preserve original-built `.o` inputs under the relink workflow and keep the phase-06 schema for `case_id`, `owner_phase`, `coverage_class`, `object_source_kind`, fixture source, preserved-object path, required startfiles or archives, exercised surfaces, and run mode.
- Port `dlopen`/`dlsym`/`dlvsym`/`dladdr` public surfaces consistently with the existing Rust loader control-plane logic from phase 04.
- Replace remaining helper-tool wrappers owned by phase 09.
- Port wide-character, message-catalog, timezone, sunrpc, login, and auxiliary runtime surfaces and their tests.
- Materialize phase-09 `source_path == null` assets through the generalized placeholder/scaffold path, especially `catgets` `.cat` fixtures, header-check placeholders, gmon compare files, and other generated test artifacts.
- Explicitly port the `catgets` and `gnulib` subtrees named in `safe/generated/baseline/test-port-plan.json`.
- Keep normalized `safe/tests/sysdeps/**` and `safe/tests/sysdeps-x86_64/**` destinations aligned with the manifest.
- Carry `safe/tests/manual/.gitkeep` forward as the owned zero-entry sentinel for a destination with no executable tests.
- Shared script files remain single checked-in assets; mark only catalog rows that point at `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py` as `ported` in `safe/tests/manifest.toml`.
- Update `cve-status.toml` for remaining rows owned here, including `memcmp x32`, `sunrpc svc_run`, and any still-open regex, glob, or resource rows that land in this phase's owned source surface.

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
  - Purpose: Verify cumulative installed-package coverage after remaining DSO and helper-tool cutovers, including required-package/debug coherence, prior installed surfaces, and new dev/time tools.
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

**Success Criteria**

- All four `check_09_*` phases pass in order.
- Phase-09 helper tools no longer ship as temporary fallback wrappers.
- Remaining non-final DSOs pass `check-abi`.
- The phase-09 relink smoke proves phase-06 through phase-09 entries in `safe/generated/baseline/link-compat-corpus.json` link and run against the safe install root.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, `locale-tools`, and `dev-and-time-tools`.
- `check-owned-tests --owner-phase impl_09_math_and_aux_dsos` proves every phase-owned catalog row is materialized, marked `ported`, and passing where executable, including `catgets`, `gnulib`, `mathvec`, shared script rows, and normalized `safe/tests/sysdeps/**` plus `safe/tests/sysdeps-x86_64/**`.
- `safe/tests/manual/.gitkeep` remains tracked and explicitly covered even though the destination has no executable tests.

**Git Commit Requirement**

The implementer must commit all phase-owned work to git before yielding.
