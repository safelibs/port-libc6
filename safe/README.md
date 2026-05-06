# safe

This workspace is the committed safe-port baseline, packaging surface, staged upstream-build harness, and final phase-10 libc6 package cutover through `impl_10_final_fixup_and_audit`.

## Commands

- `cargo run -p xtask -- ingest-baseline --source ../original --build work/original-build --dependents ../dependents.json --cves ../relevant_cves.json --verify`
- `cargo run -p xtask -- stage-upstream-build --source original --build work/original-build`
- `cargo run -p xtask -- build --target amd64 --profile release`
- `cargo run -p xtask -- check-abi --all-dsos`
- `cargo run -p xtask -- link-compat-smoke`
- `cargo run -p xtask -- test-package-install --smoke-set backend-payload-closure --deb-dir work/debs`
- `cargo run -p xtask -- test-package-install --smoke-set dev-link-artifacts --deb-dir work/debs`
- `cargo run -p xtask -- audit-safety --verify-policy --deny-unreviewed-unsafe --deny-untracked-fallback-c --deny-shipped-temporary-fallback-binaries --deny-shipped-private-backend-dsos --require-cve-disposition --require-package-scope-clean`

## Generated Baselines

- `5584` cataloged upstream test entries keyed by stable `catalog_id`
- `1695` required-package file records plus deferred and testroot-only classifications
- `40` tracked security entries in `safe/upstream-compat/cve-status.toml`
- `16` checked-in dependents validated in place from `dependents.json`

## Phase Notes

- Original repository inputs under `original/**`, `safe/work/original-build/**`, `dependents.json`, and the CVE manifests remain authoritative.
- `safe/generated/baseline/link-compat-corpus.json` is the single authoritative relink-case oracle; later phases extend it in place instead of rediscovering coverage from the build tree.
- `safe/generated/baseline/test-port-plan.json` is the committed ownership map for later test porting work.
- `safe/generated/baseline/fallback-c-inventory.json` and `safe/upstream-compat/safety-policy.toml` are the single safety-audit ledgers for fallback, unsafe, CVE, and final backend-removal closure.
- The only shipped binary package set is `libc6`, `libc6-dev`, `libc6-dbg`, `libc-bin`, `libc-dev-bin`, `locales`, and `nscd`.
- Phase 10 removes shipped private baseline backend DSOs, rejects temporary fallback binaries in final safety mode, and cuts code-bearing `libc6-dev` startfiles, archives, development links, and audit helpers over to safe-owned provenance.
- The root `test-original.sh` harness installs the local safe apt repository, installs the safe package set, smoke-tests the 16 runtime dependents from `dependents.json`, and source-builds `strace`, `valgrind`, and `libvirt` inside Ubuntu 24.04.
- Deferred for later phases: `libc-devtools`, `libc-l10n`, `locales-all`, documentation packages, source packages, and udebs.
- The committed packaging surface lives under `safe/debian/**` and `safe/generated/packaging/package-build-manifest.json`; later phases must not read undeclared packaging assets from `original/debian/**`.
