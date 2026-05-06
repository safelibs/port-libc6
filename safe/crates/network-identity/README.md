# network-identity

Phase 7 keeps bounded resolver helpers and NSS/nscd state primitives in Rust.
The generated DSO veneers link these objects for phase-owned symbols and
forward the remaining ABI surface to explicitly inventoried private backends.
