# libc6 Runtime Port

Phase 6 keeps the startup port in place, carries the low-level runtime exports under `safe/crates/libc6/src/sys/**`, and cuts the first libc-family public DSO payloads over to safe-built artifacts.
