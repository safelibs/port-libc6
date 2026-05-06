# Safe-Side Upstream Test Tree

This tree is the committed phase-owned copy of upstream tests and fixtures used
by the safe libc port.

- `safe/tests/support/**` mirrors the committed upstream support subtree.
- `safe/tests/manifest.toml` is the authoritative phase ownership ledger for the
  copied tests.
- Phase 7 adds the hesiod, inet, nis, nss, resolv, socket, nscd sentinel,
  shared script, and normalized sysdeps entries while preserving later committed
  port statuses in place.

