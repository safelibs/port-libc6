# libc6 Runtime Port

Phase 8 keeps the startup port in place, carries the low-level runtime exports under `safe/crates/libc6/src/sys/**`, and extends the safe-built public DSO cutover through libBrokenLocale while the locale and iconv helper entrypoints move onto committed Rust frontends.
