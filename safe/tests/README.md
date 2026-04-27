# Safe-Side Upstream Test Tree

This tree is the committed phase-owned copy of upstream tests and fixtures used
by the safe libc port.

- `safe/tests/support/**` mirrors the committed upstream support subtree.
- `safe/tests/manifest.toml` is the authoritative phase ownership ledger for the
  copied tests.
- Phase 6 adds the io, stdio-common, string, dirent, time, timezone, assert,
  ctype, and termios-owned entries while preserving the earlier committed phase
  ownership in place.

