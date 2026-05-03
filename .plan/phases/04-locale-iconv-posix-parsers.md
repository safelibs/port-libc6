# Phase Name
Locale, Iconv, Localedata, Conform, and POSIX Parser Cutover

## Implement Phase ID
`impl_08_locale_iconv_posix_parsers`

## Preexisting Inputs
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
  - `safe/generated/security/relevant-cves-index.json`
- Existing authoritative build, package, and test inputs that phase 08 must extend in place rather than rediscover:
  - `original/**`
  - `safe/scripts/stage-original-build.sh`
  - `safe/generated/packaging/package-build-manifest.json`
  - `safe/generated/baseline/test-catalog.json`
  - `safe/generated/baseline/test-port-plan.json`
  - `safe/generated/baseline/fallback-c-inventory.json`
- Existing shared harness inputs already materialized on disk and still authoritative, especially `safe/tests/support/**`, `safe/tests/support/glibcpp.py`, `safe/tests/include/**`, `safe/tests/bits/**`, `safe/tests/top-level/{Makefile,Makeconfig,Makerules}`, `safe/tests/test-skeleton.c`, `safe/tests/c++-types.data`, and the shared script assets under `safe/tests/scripts/**`.
- Existing copied-test roots already materialized before phase 08 and touched again here must be updated in place rather than rematerialized:
  - `safe/tests/sysdeps/**`
  - `safe/tests/sysdeps-x86_64/**`
  - `safe/tests/sysdeps-linux-x86_64/**`
- Preserve the consume-existing-artifacts contract: extend the committed relink corpus, package manifests, tests, and ledgers in place. Do not regenerate `safe/generated/baseline/test-port-plan.json` or replace already-materialized shared assets.

## New Outputs
- Rust-backed locale, iconv, and parser implementations in the public libc surface.
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

## File Changes
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
  - new locale, iconv, or parser crates or modules under `safe/crates/**`
  - generated charset tables or build helpers under `safe/**` as needed
  - `safe/Cargo.toml`
  - `safe/Cargo.lock`
- Tests and ledgers:
  - `safe/tests/manifest.toml`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - phase-owned files under `safe/tests/conform/**`, `safe/tests/posix/**`, `safe/tests/localedata/**`, `safe/tests/iconvdata/**`, `safe/tests/iconv/**`, `safe/tests/locale/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`
  - `safe/tests/po/.gitkeep`
  - `safe/tests/scripts/check-wrapper-headers.py`
  - `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/safety-policy.toml`

## Implementation Details
- Replace the remaining locale and iconv temporary wrappers with real shipped assets. Prefer Rust executables for `iconv`, `iconvconfig`, `localedef`, and `locale`.
- Ship helper scripts such as `locale-gen`, `update-locale`, `validlocale`, `install-language-pack`, and `remove-language-pack` directly as first-class scripts rather than routing them through temporary fallback wrappers.
- Whenever this phase changes a shipped helper binary, helper script, locale data path, or public package payload entry, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as the corresponding `safe/generated/baseline/package-files/*.json` edits.
- Extend `safe/xtask/src/commands/test_package_install.rs` so the `locale-tools` smoke set validates the installed package payload, helper scripts, and locale maintainer-script behavior from the generated `.deb` outputs rather than from the build tree.
- Port parser-heavy libc logic into safe Rust with explicit malformed-input handling and guaranteed forward progress. Do not use assertions for externally reachable malformed input, and preserve glibc error codes, locale search order, and file-format semantics.
- Ensure generated or loaded locale databases remain package-compatible with the existing Debian maintainer scripts and install paths.
- Extend `safe/generated/baseline/link-compat-corpus.json` in place so it exercises preserved original-built objects or original-sysroot fixtures that reference locale, iconv, and parser-facing public surfaces, including representative `setlocale` or `newlocale`, `iconv`, `regcomp` or `regexec`, `fnmatch`, `glob`, `wordexp`, and `libBrokenLocale` coverage where stable object inputs exist.
- Port the copied tests and keep the `conform` subtree authoritative. Do not invent an alternate parser test corpus.
- Carry the `safe/tests/po/.gitkeep` zero-entry sentinel forward exactly as assigned in `safe/generated/baseline/test-port-plan.json`, and treat the shared `check-wrapper-headers.py` and `check-obsolete-constructs.py` files as existing inputs whose phase-owned manifest rows become `ported` in place.
- Update `safe/upstream-compat/cve-status.toml` for all iconv state-machine rows, `fnmatch`, regex compiler and engine rows, `wordexp`, `glob`, `locale path handling`, and any out-of-scope `crypt` row with an explicit package-scope rationale.

## Verification Phases
### `check_08_locale_tests`
- `phase_id`: `check_08_locale_tests`
- `type`: `check`
- `bounce_target`: `impl_08_locale_iconv_posix_parsers`
- `purpose`: Validate the large phase-owned test tree and run the locale, iconv, and POSIX parser tests.
- `commands`:
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

### `check_08_locale_abi`
- `phase_id`: `check_08_locale_abi`
- `type`: `check`
- `bounce_target`: `impl_08_locale_iconv_posix_parsers`
- `purpose`: Verify the libc and `libBrokenLocale` symbol surface, original-object relink compatibility, and installed headers after the locale and iconv parser cutover.
- `commands`:
```bash
cd safe
cargo run -p xtask -- stage-upstream-build --source ../original --build work/original-build
cargo run -p xtask -- build --target amd64 --profile release
cargo run -p xtask -- check-abi --dso libc --dso libBrokenLocale
cargo run -p xtask -- link-compat-smoke --install-root work/install-root --build-root work/original-build
cargo run -p xtask -- check-headers --root work/install-root --lang c --lang c++
```

### `check_08_locale_packages`
- `phase_id`: `check_08_locale_packages`
- `type`: `check`
- `bounce_target`: `impl_08_locale_iconv_posix_parsers`
- `purpose`: Verify cumulative installed-package coverage after the locale cutover, including required-package and debug coherence, earlier libc-family and network payloads, and the new locale and iconv helper scripts and data files.
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
```

### `check_08_locale_safety`
- `phase_id`: `check_08_locale_safety`
- `type`: `check`
- `bounce_target`: `impl_08_locale_iconv_posix_parsers`
- `purpose`: Enforce CVE disposition and reviewed unsafe and fallback coverage for parsers and locale state machines.
- `commands`:
```bash
cd safe
cargo run -p xtask -- audit-safety \
  --deny-unreviewed-unsafe \
  --deny-untracked-fallback-c \
  --require-cve-disposition
```

## Success Criteria
- All locale and iconv helper paths owned by phase 08 are no longer temporary fallback binaries.
- The phase-08 relink smoke proves that the phase-06 through phase-08 entries committed in `safe/generated/baseline/link-compat-corpus.json` still link and run against the safe install root after the public libc and `libBrokenLocale` cutover.
- The locale packages install and run from `.deb` outputs, not just from a build tree.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, and `locale-tools`, so required-package and debug coherence plus previously cut-over installed surfaces stay live after the locale and iconv cutover.
- `check-owned-tests --owner-phase impl_08_locale_iconv_posix_parsers` proves that every phase-owned catalog row is materialized, marked `ported`, and that the executable subset passes under the Rust-backed install root.
- The tracked `safe/tests/po/.gitkeep` sentinel remains explicitly covered even though that destination has no executable tests.
- `audit-safety` passes with reviewed unsafe and fallback coverage plus current CVE dispositions.

## Git Commit Requirement
Commit all phase-scoped changes to git before yielding. The commit must include the locale and iconv implementation and packaging work, the manifest and ledger updates, the relink corpus extensions, the phase-owned tests, and the tracked `safe/tests/po/.gitkeep` sentinel.
