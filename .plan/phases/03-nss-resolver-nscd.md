# Phase Name
NSS, Resolver, Inet/Socket Glue, and Nscd Client/Tool Cutover

## Implement Phase ID
`impl_07_nss_resolver_nscd`

## Preexisting Inputs
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
  - `safe/generated/security/relevant-cves-index.json`
- Existing authoritative build, package, and test inputs that phase 07 must extend in place rather than rediscover:
  - `original/**`
  - `safe/scripts/stage-original-build.sh`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
- Existing shared harness inputs already materialized on disk and still authoritative, especially `safe/tests/support/**`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets under `safe/tests/scripts/**`.
- Existing copied-test roots already materialized before phase 07 and touched again here must be updated in place rather than rematerialized:
  - `safe/tests/resolv/**`
  - `safe/tests/sysdeps/**`
- Preserve the consume-existing-artifacts contract: extend the committed relink corpus, tests, manifests, and ledgers in place rather than rediscovering ownership or regenerating authoritative inputs.

## New Outputs
- Rust-backed `libresolv`, `libanl`, `libnsl`, and `libnss_*` public DSOs or DSO veneers that run phase-owned symbols in Rust and forward the remainder to private baseline backends.
- A Rust or first-class shipped implementation for `/usr/bin/getent` and `/usr/sbin/nscd`, eliminating those temporary fallback wrappers.
- Updated committed relink fixtures, referenced by `safe/generated/baseline/link-compat-corpus.json`, for the phase-07 `libresolv`, `libnss_*`, and `libanl` original-sysroot object cases.
- Ported tests for the 156 phase-owned `hesiod`, `inet`, `nis`, `nss`, `resolv`, `socket`, `safe/tests/sysdeps/**`, and shared `safe/tests/scripts/{check-wrapper-headers.py,check-obsolete-constructs.py}` entries.
- Updated network package manifests, install manifests, and closed or dispositioned CVE rows for resolver, NSS, and nscd-client issues.

## File Changes
- Build, package, and install:
  - `safe/xtask/src/commands/build.rs`
  - `safe/xtask/src/commands/install_root.rs`
  - `safe/xtask/src/commands/package_deb.rs`
  - `safe/xtask/src/commands/link_compat_smoke.rs`
  - `safe/xtask/src/commands/test_package_install.rs`
  - `safe/generated/baseline/link-compat-corpus.json`
  - the committed relink fixtures referenced by `safe/generated/baseline/link-compat-corpus.json` for the phase-07 `libresolv`, `libnss_*`, and `libanl` original-sysroot cases
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
  - `safe/Cargo.toml`
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
  - `safe/tests/scripts/check-wrapper-headers.py`
  - `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/upstream-compat/safety-policy.toml`

## Implementation Details
- Extend the phase-06 forwarding-veneer design to the network-identity DSOs and the corresponding libc export set.
- Whenever this phase changes a shipped DSO, NSS module, helper binary, service unit, or configuration-file path, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits.
- Keep `safe/xtask/src/commands/link_compat_smoke.rs` authoritative for link compatibility as the network DSOs move off baseline payloads.
- Extend `safe/generated/baseline/link-compat-corpus.json` in place with phase-07-owned `libresolv`, `libnss_*`, and `libanl` relink cases, recording owner phase, object source kind, fixture source when needed, preserved-object path, coverage class, and exercised surfaces.
- Add or refresh only the committed relink fixtures referenced by that manifest for `libresolv`, `libnss_*`, and `libanl` objects originally built against the original sysroot.
- Continue requiring the relink step to consume preserved original-built objects instead of recompiling against `work/install-root`.
- Implement resolver parsing and answer validation in Rust with explicit defenses for non-answer-section confusion in reverse lookups, invalid reverse-DNS hostnames, numeric-host parsing corner cases, `if_nametoindex` or `getaddrinfo` interaction, and stub-resolver malformed-message handling.
- Replace the library-side nscd client shared-memory reads with a snapshot or generation-checked design that cannot observe torn cross-process state.
- Move `getent` and `nscd` off `RequiredToolKind::FallbackWrapper` ownership for phase 07. Prefer Rust implementations; if `nscd` remains outside Rust, make it a declared shipped asset rather than a hidden fallback wrapper.
- Port all phase-owned copied tests and mark them `ported` in `safe/tests/manifest.toml`, including the committed `hesiod` and `nis` trees, the normalized `safe/tests/sysdeps/**` destination, the shared `check-wrapper-headers.py` and `check-obsolete-constructs.py` rows, and the zero-entry sentinel `safe/tests/nscd/.gitkeep`.
- Update `safe/upstream-compat/cve-status.toml` for `nss_dns / gethostbyaddr`, `nscd client / NSS shared cache`, `getaddrinfo numeric host parsing`, `getaddrinfo / if_nametoindex`, `stub resolver`, `NSS files backend`, `nss_dns / getnetbyname`, and `nss_nis / getpwnam`.

## Verification Phases
### `check_07_network_tests`
- `phase_id`: `check_07_network_tests`
- `type`: `check`
- `bounce_target`: `impl_07_nss_resolver_nscd`
- `purpose`: Validate all phase-owned copied test entries and run the upstream network-related tests against the Rust-backed install root.
- `commands`:
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

### `check_07_network_abi`
- `phase_id`: `check_07_network_abi`
- `type`: `check`
- `bounce_target`: `impl_07_nss_resolver_nscd`
- `purpose`: Verify ABI, versioning, original-object relink compatibility, and runtime linkage for the network-identity DSOs and their libc exports.
- `commands`:
```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check-abi --dso libanl --dso libnsl --dso libnss_compat --dso libnss_dns --dso libnss_files --dso libnss_hesiod --dso libresolv --dso libc
cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
```

### `check_07_network_packages`
- `phase_id`: `check_07_network_packages`
- `type`: `check`
- `bounce_target`: `impl_07_nss_resolver_nscd`
- `purpose`: Verify cumulative installed-package coverage after the network cutover, including required-package and debug coherence, libc-family payload provenance, earlier loader and runtime entrypoints, and the new `getent`, `nscd`, and NSS surfaces.
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
```

### `check_07_network_safety`
- `phase_id`: `check_07_network_safety`
- `type`: `check`
- `bounce_target`: `impl_07_nss_resolver_nscd`
- `purpose`: Enforce reviewed unsafe and fallback coverage and CVE disposition for the resolver, NSS, and nscd-client surfaces.
- `commands`:
```bash
cd safe
cargo run -p xtask -- audit-safety \
  --deny-unreviewed-unsafe \
  --deny-untracked-fallback-c \
  --require-cve-disposition
```

## Success Criteria
- `getent` and `nscd` no longer ship as temporary fallback wrappers in `safe/crates/libc-support-tools/src/fallback.rs`.
- The network DSOs pass `check-abi`.
- The phase-07 relink smoke still uses the committed phase-06 and phase-07 entries in `safe/generated/baseline/link-compat-corpus.json` and passes after the `libresolv` and `libnss_*` cutover.
- Any phase-07 relink-fixture changes are limited to the committed fixtures referenced by `safe/generated/baseline/link-compat-corpus.json` for the `libresolv`, `libnss_*`, and `libanl` original-sysroot cases.
- `check-owned-tests --owner-phase impl_07_nss_resolver_nscd` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset passes under the Rust-backed install root.
- The tracked `safe/tests/nscd/.gitkeep` sentinel remains explicitly accounted for even though that destination has no executable `run-original-tests` coverage.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, and `network-tools`, so required-package and debug coherence plus earlier installed surfaces stay live after the network DSO cutover.
- `audit-safety` passes with explicit reviewed unsafe and fallback coverage and current CVE dispositions.

## Git Commit Requirement
Commit all phase-scoped changes to git before yielding. The commit must include the network DSO and tool cutover work, the manifest and ledger updates, the relink corpus extensions, the phase-owned tests, and the tracked `safe/tests/nscd/.gitkeep` sentinel.
