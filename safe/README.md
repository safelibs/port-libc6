# safe

This workspace is the committed safe-port baseline, packaging surface, staged upstream-build harness, and first libc-family public cutover through `impl_06_io_stdio_string_path`.

## Commands

- `cargo run -p xtask -- ingest-baseline --source ../original --build work/original-build --dependents ../dependents.json --cves ../relevant_cves.json --verify`
- `cargo run -p xtask -- stage-upstream-build --source original --build work/original-build`
- `cargo run -p xtask -- check-owned-tests --owner-phase impl_06_io_stdio_string_path`
- `cargo run -p xtask -- audit-safety --verify-policy`

## Generated Baselines

- `5584` cataloged upstream test entries keyed by stable `catalog_id`
- `1688` required-package file records plus deferred and testroot-only classifications
- `40` tracked security entries in `safe/upstream-compat/cve-status.toml`
- `16` checked-in dependents validated in place from `dependents.json`

## Phase Notes

- Original repository inputs under `original/**`, `safe/work/original-build/**`, `dependents.json`, and the CVE manifests remain authoritative.
- `safe/generated/baseline/link-compat-corpus.json` is the single authoritative relink-case oracle; later phases extend it in place instead of rediscovering coverage from the build tree.
- `safe/generated/baseline/test-port-plan.json` is the committed ownership map for later test porting work.
- `safe/generated/baseline/fallback-c-inventory.json` and `safe/upstream-compat/safety-policy.toml` are the single safety-audit ledgers that later phases must extend in place.
- The only shipped phase-3 binary package set is `libc6`, `libc6-dev`, `libc6-dbg`, `libc-bin`, `libc-dev-bin`, `locales`, and `nscd`.
- Phase 6 moves the public `libc6` runtime DSOs and the phase-owned public `libc6-dev` `.so` link names onto the safe build path while keeping any remaining baseline code under explicit private backend inventory paths.
- Deferred for later phases: `libc-devtools`, `libc-l10n`, `locales-all`, documentation packages, source packages, and udebs.
- The committed packaging surface lives under `safe/debian/**` and `safe/generated/packaging/package-build-manifest.json`; later phases must not read undeclared packaging assets from `original/debian/**`.
