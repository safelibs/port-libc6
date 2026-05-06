# libpthread Runtime State

Phase 6 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while the first libc-family DSO cutover reuses the safe-built packaging path.
