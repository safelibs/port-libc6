# NSS Resolver NSCD

**Phase Name**

NSS, resolver, inet/socket glue, and nscd client/tool cutover

**Implement Phase ID**

`impl_07_nss_resolver_nscd`

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
  - `safe/generated/baseline/abi/libc.json`
  - `safe/generated/version-scripts/*.map`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/nscd.json`
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
  - `dependents.json`
  - `relevant_cves.json`
- If already present, the ignored derived `safe/work/original-build/**` tree. Otherwise checkers must run `stage-upstream-build` before consuming it; it is not a required preexisting workflow input.

**New Outputs**

- Rust-backed `libresolv`, `libanl`, `libnsl`, and `libnss_*` public DSOs or DSO veneers that run phase-owned symbols in Rust and forward only remaining unported symbols to private baseline backends.
- Rust or first-class shipped implementations for `/usr/bin/getent` and `/usr/sbin/nscd`, eliminating temporary fallback wrappers for those tools.
- Ported tests for the 156 phase-owned `hesiod`, `inet`, `nis`, `nss`, `resolv`, `socket`, `safe/tests/sysdeps/**`, and shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` entries.
- A tracked `safe/tests/nscd/.gitkeep` sentinel for the zero-entry `safe/tests/nscd` destination recorded in `safe/generated/baseline/test-port-plan.json`.
- Updated network package manifests, install manifests, relink corpus entries, fallback inventory, package-scope ledger, safety policy, and CVE dispositions.

**File Changes**

- Build, package, install, and verification:
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
  - new or expanded Rust tool code for `getent` and `nscd`
- Network implementation:
  - new crates or modules for resolver and NSS logic under `safe/crates/**`
  - `safe/Cargo.toml` if new workspace members are added
  - `safe/Cargo.lock`
- Debian and package metadata:
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
  - `safe/tests/nscd/.gitkeep`
  - shared phase-owned manifest rows for `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`

**Implementation Details**

- Extend the phase-06 forwarding-veneer design to network-identity DSOs and the corresponding libc export set.
- Whenever this phase changes a shipped DSO, NSS module, helper binary, service unit, or configuration-file path, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits.
- Keep package-smoke coverage cumulative: `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, and new `network-tools` must verify the committed authorities.
- Extend `safe/generated/baseline/link-compat-corpus.json` in place with phase-07-owned `libresolv`, `libnss_*`, and `libanl` cases. Each case must preserve the phase-06 schema: `case_id`, `owner_phase`, `coverage_class`, `object_source_kind`, fixture source when needed, preserved-object path under `safe/work/link-smoke/original-objects/**`, required startfiles or archives, exercised surfaces, and run mode.
- Add or refresh only the committed relink fixtures referenced by that manifest for objects originally built against the original sysroot. The relink step must consume preserved original-built objects and must not recompile against `work/install-root`.
- Implement resolver parsing and answer validation in Rust with defenses for non-answer-section confusion in reverse lookups, invalid reverse-DNS hostnames, numeric-host parsing corner cases, `if_nametoindex`/`getaddrinfo` interaction, and stub-resolver malformed-message handling.
- Replace library-side nscd client shared-memory reads with a snapshot or generation-checked design that cannot observe torn cross-process state.
- Move `getent` and `nscd` off `RequiredToolKind::FallbackWrapper` ownership.
- `getent` should become a Rust or direct first-class implementation. `nscd` may remain a daemon implemented outside Rust only if it is declared as a non-temporary shipped asset rather than a hidden fallback wrapper; the preferred result is a Rust-fronted daemon or direct packaged binary with explicit status.
- Port all phase-owned copied tests and mark them `ported` in `safe/tests/manifest.toml`. This includes `hesiod`, `inet`, `nis`, `nss`, `resolv`, `socket`, normalized `safe/tests/sysdeps/**`, shared script manifest rows, and `safe/tests/nscd/.gitkeep`.
- Shared script files already exist; update manifest rows pointing at them in place instead of duplicating files.
- Update `cve-status.toml` for `nss_dns / gethostbyaddr`, `nscd client / NSS shared cache`, `getaddrinfo numeric host parsing`, `getaddrinfo / if_nametoindex`, `stub resolver`, `NSS files backend`, `nss_dns / getnetbyname`, and `nss_nis / getpwnam`.

**Verification Phases**

- `check_07_network_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_07_nss_resolver_nscd`
  - Purpose: Validate all phase-owned copied test entries and run upstream network-related tests against the Rust-backed install root.
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
  - Purpose: Enforce reviewed unsafe/fallback coverage and CVE disposition for resolver, NSS, and nscd-client surfaces.
  - Commands:
    ```bash
    cd safe
    cargo run -p xtask -- audit-safety \
      --deny-unreviewed-unsafe \
      --deny-untracked-fallback-c \
      --require-cve-disposition
    ```

**Success Criteria**

- All four `check_07_*` phases pass in order.
- `getent` and `nscd` no longer ship as temporary fallback wrappers.
- Network DSOs pass `check-abi`.
- The relink smoke uses the cumulative entries committed in `safe/generated/baseline/link-compat-corpus.json` and passes after the `libresolv`/`libnss_*` cutover.
- `check-owned-tests --owner-phase impl_07_nss_resolver_nscd` proves every phase-owned catalog row is materialized, marked `ported`, and passing where executable.
- `safe/tests/nscd/.gitkeep` is tracked and explicitly accounted for even though that destination has no executable `run-original-tests` coverage.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, and `network-tools`, preserving required-package/debug coherence and earlier installed surfaces.

**Git Commit Requirement**

The implementer must commit all phase-owned work to git before yielding.
