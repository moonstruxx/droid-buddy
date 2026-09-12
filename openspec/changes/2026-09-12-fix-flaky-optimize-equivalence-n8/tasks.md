# Tasks

## 1. Pin the schema view in the flaky weighted-equivalence test

- [ ] In `src/optimize.rs`, `weighted_brute_force_equivalence_n8` (and its sibling that mixes a test-built eval with engine calls, if any): pin the calling thread's schema view (`crate::schema::set_test_schema(Some(schema))`) for the duration of the test and restore it (`set_test_schema(None)`) before returning, so the brute-force `eval` and `generate_candidates_weighted`'s internal `load_schema()` see the same `&'static Schema` instance even when a parallel schema test swaps the process-global `SCHEMA_CACHE` (design decision from issue droid_tui-m25: fix the nondeterminism at its source rather than weakening the assertion to an epsilon comparison).
- [ ] Acceptance: the test still asserts exact `LatencySummary` equality (no epsilon), and repeated full-suite parallel runs no longer flake on `avg` deltas ~1e-5.
- [ ] Verify: `cargo test --lib optimize::tests::weighted_brute_force_equivalence_n8` passes; full verification gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) exits 0.