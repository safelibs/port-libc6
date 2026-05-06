# libpthread Runtime State

Phase 8 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while locale and parser test coverage runs through the shared install-root harness.
