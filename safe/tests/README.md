# Safe-Side Upstream Test Tree

This tree is the committed phase-owned copy of upstream tests and fixtures used
by the safe libc port.

- `safe/tests/support/**` mirrors the committed upstream support subtree.
- `safe/tests/manifest.toml` is the authoritative phase ownership ledger for the
  copied tests.
- Phase 6 adds the stdio, stdlib, libio, string, io, time, dirent, assert,
  ctype, termios, timezone, generated placeholder, shared script, and normalized
  sysdeps entries while preserving later committed port statuses in place.

