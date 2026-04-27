# libpthread Runtime State

Phase 7 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while later network-facing DSO cutovers reuse the same safe-built packaging path.
