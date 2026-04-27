# core-runtime

Phase 7 keeps low-level syscall wrappers, errno and TLS state, futex helpers, allocator entrypoints, signal bookkeeping, and entropy interfaces under `safe/crates/core-runtime/src/**` while the libc-family package cutover extends through the network-facing DSOs.
