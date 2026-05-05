# libpthread Runtime State

Phase 9 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while the remaining math and auxiliary DSO cutovers reuse the same safe-built packaging path.
