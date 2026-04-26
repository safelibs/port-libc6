# Upstream-Compatible Harness

This tree is the committed harness for validating ported safe-side test sources
against the checked-in upstream build outputs while the runtime remains hybrid.

- `safe/upstream-tests/build/` is transient scratch state only.
- `safe/work/original-build/` is the staged upstream build tree consumed by the
  harness and smoke checks.
- `cargo run -p xtask -- run-original-tests ...` populates that build tree from
  the committed safe test sources and the checked-in upstream build artifacts.
- Phase 5 extends that committed test tree with the runtime, threads, malloc,
  signal, setjmp, misc, nptl_db, and entropy-owned coverage without inventing a
  parallel workflow.

