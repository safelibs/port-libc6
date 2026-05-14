# Locale Iconv Posix

**Phase Name**

Locale, iconv, localedata, conform, and POSIX parser cutover

**Implement Phase ID**

`impl_08_locale_iconv_posix_parsers`

**Preexisting Inputs**

- All outputs from `impl_07_nss_resolver_nscd` and its passing checks.
- Existing authoritative inputs:
  - `safe/generated/baseline/link-compat-corpus.json`
  - `safe/generated/baseline/abi/libBrokenLocale.json`
  - `safe/generated/baseline/abi/libc.json`
  - `safe/generated/version-scripts/*.map`
  - `safe/generated/baseline/package-files/libc6.json`
  - `safe/generated/baseline/package-files/libc-bin.json`
  - `safe/generated/baseline/package-files/locales.json`
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

- Rust-backed locale, iconv, and parser implementations in the public libc surface.
- First-class shipped implementations for `/usr/bin/iconv`, `/usr/sbin/iconvconfig`, `/usr/bin/localedef`, `/usr/bin/locale`, `/usr/sbin/locale-gen`, `/usr/sbin/update-locale`, `/usr/sbin/validlocale`, `/usr/share/locales/install-language-pack`, and `/usr/share/locales/remove-language-pack`.
- Ported test tree for the 1,393 phase-owned entries.
- A tracked `safe/tests/po/.gitkeep` sentinel for the zero-entry `safe/tests/po` destination recorded in `safe/generated/baseline/test-port-plan.json`.
- Updated locale package manifests, install manifests, relink corpus entries, fallback inventory, package-scope ledger, safety policy, and CVE dispositions.

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
  - new locale, iconv, or parser crates or modules under `safe/crates/**`
  - generated charset tables or build helpers under `safe/**` as needed
  - `safe/Cargo.toml`
  - `safe/Cargo.lock`
- Tests and ledgers:
  - `safe/tests/manifest.toml`
  - `safe/xtask/src/commands/check_owned_tests.rs`
  - phase-owned files under `safe/tests/conform/**`, `safe/tests/posix/**`, `safe/tests/localedata/**`, `safe/tests/iconvdata/**`, `safe/tests/iconv/**`, `safe/tests/locale/**`, `safe/tests/sysdeps/**`, `safe/tests/sysdeps-x86_64/**`, and `safe/tests/sysdeps-linux-x86_64/**`
  - `safe/tests/po/.gitkeep`
  - shared phase-owned manifest rows for `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py`
  - `safe/upstream-compat/port-status.toml`
  - `safe/upstream-compat/cve-status.toml`
  - `safe/generated/baseline/fallback-c-inventory.json`
  - `safe/upstream-compat/safety-policy.toml`

**Implementation Details**

- Replace remaining locale and iconv temporary wrappers with real shipped assets.
- Prefer Rust executables for true programs: `iconv`, `iconvconfig`, `localedef`, and `locale`.
- Ship helper scripts such as `locale-gen`, `update-locale`, `validlocale`, `install-language-pack`, and `remove-language-pack` directly as first-class scripts instead of routing them through the fallback wrapper mechanism.
- Whenever this phase changes a shipped helper binary, helper script, locale data path, or public package payload entry, update `safe/generated/install-manifests/required-packages.json` and `safe/generated/install-manifests/test-install-root.json` in the same commit as corresponding `safe/generated/baseline/package-files/*.json` edits.
- Keep package-smoke coverage cumulative: `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, and new `locale-tools` must verify the committed authorities.
- Extend `test_package_install.rs` so `locale-tools` validates installed package payloads, helper scripts, and locale maintainer-script behavior from generated `.deb` outputs rather than from the build tree. Preserve required-package/debug checks in `basic-required-packages`.
- Port parser-heavy libc logic into safe Rust with malformed-input handling and guaranteed forward progress. Do not use assertions for externally reachable malformed input.
- Preserve glibc error codes, locale search order, and file-format semantics.
- Ensure generated or loaded locale databases remain package-compatible with existing Debian maintainer scripts and install paths.
- Extend `safe/generated/baseline/link-compat-corpus.json` in place with preserved original-built objects or original-sysroot fixtures that reference locale, iconv, and parser-facing public surfaces. Include representative `setlocale`/`newlocale`, `iconv`, `regcomp`/`regexec`, `fnmatch`, `glob`, `wordexp`, and `libBrokenLocale` coverage where stable object inputs exist.
- Preserve the relink corpus schema from phase 06 for every new case: `case_id`, `owner_phase`, `coverage_class`, `object_source_kind`, fixture source when applicable, preserved-object path, required startfiles or archives, exercised surfaces, and run mode.
- Port copied tests and keep the `conform` subtree authoritative; do not invent an alternate parser test corpus.
- Carry `safe/tests/po/.gitkeep` forward exactly as assigned by `test-port-plan.json`.
- Treat shared `safe/tests/scripts/check-wrapper-headers.py` and `safe/tests/scripts/check-obsolete-constructs.py` as existing inputs whose phase-owned manifest rows become `ported` in place.
- Update `cve-status.toml` for all iconv state-machine rows, `fnmatch`, regex compiler and engine rows, `wordexp`, `glob`, locale path handling, and any `crypt` row that is out of package scope with an explicit `not-applicable` rationale if Ubuntu's libc6 package no longer ships the relevant implementation.

**Verification Phases**

- `check_08_locale_tests`
  - Type: `check`
  - Fixed `bounce_target`: `impl_08_locale_iconv_posix_parsers`
  - Purpose: Validate the large phase-owned test tree and run locale/iconv/POSIX parser tests.
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
  - Purpose: Verify cumulative installed-package coverage after the locale cutover, including required-package/debug coherence, earlier libc-family and network payloads, and new locale/iconv helper scripts and data files.
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

**Success Criteria**

- All four `check_08_*` phases pass in order.
- Locale/iconv helper paths owned by phase 08 no longer ship as temporary fallback binaries.
- The phase-08 relink smoke proves phase-06 through phase-08 entries in `safe/generated/baseline/link-compat-corpus.json` still link and run against the safe install root after the libc/libBrokenLocale cutover.
- Locale packages install and run from `.deb` outputs, not just from the build tree.
- The package verifier reruns `basic-required-packages`, `libc-family-cutover`, `loader-tools`, `runtime-tools`, `network-tools`, and `locale-tools`.
- `check-owned-tests --owner-phase impl_08_locale_iconv_posix_parsers` proves every phase-owned catalog row is materialized, marked `ported`, and passing where executable.
- `safe/tests/po/.gitkeep` remains tracked and explicitly covered even though the destination has no executable tests.

**Git Commit Requirement**

The implementer must commit all phase-owned work to git before yielding.
