# libc6 Runtime Port

Phase 9 keeps the startup port in place, carries the low-level runtime exports under `safe/crates/libc6/src/sys/**`, and extends the safe-built public DSO cutover through libdl, libm, libmvec, libpcprofile, librt, and libutil while the remaining dev/time helper entrypoints move onto committed Rust frontends.
