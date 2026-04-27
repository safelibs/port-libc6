# libpthread Runtime State

Phase 8 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while later locale and math cutovers reuse the same safe-built packaging path.
