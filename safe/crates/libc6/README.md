# libc6 Runtime Port

Phase 5 keeps the startup port in place and adds low-level runtime exports under `safe/crates/libc6/src/sys/**` for errno, entropy, memory, signal, thread, and setjmp glue.
