# Safe-Side Upstream Test Tree

This tree is the committed phase-owned copy of upstream tests and fixtures used
by the safe libc port.

- `safe/tests/support/**` mirrors the committed upstream support subtree.
- `safe/tests/manifest.toml` is the authoritative phase ownership ledger for the
  copied tests.
- Phase 5 adds the runtime-owned test set under `safe/tests/misc/**`,
  `safe/tests/malloc/**`, `safe/tests/nptl/**`, `safe/tests/nptl_db/**`,
  `safe/tests/signal/**`, `safe/tests/setjmp/**`, and the entropy-focused
  `safe/tests/stdlib/**` entries.

