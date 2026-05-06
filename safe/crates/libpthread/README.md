# libpthread Runtime State

Phase 7 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while network test coverage runs through the shared install-root harness.
