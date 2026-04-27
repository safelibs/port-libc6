# libpthread Runtime State

Phase 6 keeps the Rust-side pthread bookkeeping, futex-backed synchronization helpers, and setxid coordination under `safe/crates/libpthread/src/**` while the shipped libpthread payload moves to the safe-built public DSO path.
