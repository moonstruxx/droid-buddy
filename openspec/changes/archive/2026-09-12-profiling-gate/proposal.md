## Why

Profiling the real worst case shows the app is fast today: melody2 (532 sections, 457 cables, 115 hw tokens) parses, validates, builds, solves, and renders instantly; the full gallery matrix renders in 0.32 s CPU; every phase is structurally bounded (solver ≤ 60 iterations, optimizer ≤ search budget). But nothing enforces that. A regression that turns parse, graph build, solve, optimize search, or the render loop into a CPU hog or an unbounded loop would hang the test suite instead of failing it. This change adds a profiling gate: budgeted, watchdog-protected phases over the real workload anchors, so a hog or a loop busts the build with the phase named and the elapsed time reported.

## What Changes

- A new integration test `tests/perf_gate.rs` runs the gated phases over a scale-anchor fixture and the full gallery render matrix, each under a named wall-clock budget.
- A `budgeted` watchdog helper runs each phase on its own thread with a `recv_timeout` ceiling; a phase that exceeds its ceiling fails the test with the phase name and elapsed time instead of hanging the suite.
- Budget constants are calibrated from the measured release baselines with a wide margin (10 s phases, 30 s matrix), so the gate never flakes on slow machines but still trips on genuine hogs and loops.
- A deep-dive script `scripts/profile-gate.sh` records a chosen phase under `perf` and renders a flamegraph into `.opencode/.tmp/profiling/`, so a busted budget produces evidence, not just a red test.
- CI runs the perf gate in release mode (`cargo test --release --test perf_gate`) so it busts on honest release timings.

## Capabilities

### New Capabilities

- `performance-gate`: budgeted, watchdog-protected execution of the scale-anchor phases and the gallery render matrix, with calibrated budgets and a deep-dive profiling workflow.

### Modified Capabilities

- `visual-validation`: the "Snapshot generation from TestBackend" requirement gains a bounded-render-throughput clause: the gallery matrix must complete within the render budget.

## Impact

- `tests/perf_gate.rs`: new integration test (the gate itself).
- `fixtures/droid_mpfs5melody2.ini`: already tracked; becomes the scale anchor (532 sections, 457 cables, 115 hw tokens).
- `scripts/profile-gate.sh`: new deep-dive profiling script.
- `.github/workflows/ci.yml`: new release-mode perf-gate step.
- `src/`: no runtime code changes. The gate exercises existing public entry points only (`Patch::from_ini_file`, `validate_patch`, `Graph::build_from_patch`, `layout::solve`, `optimize::generate_candidates`, the gallery harness).

## Non-goals

- No performance optimization of any phase. The gate measures and busts; it does not tune.
- No tight regression budgets. This is not a benchmark suite; budgets are generous margins over measured baselines.
- No changes to the solver or optimizer algorithms, iteration caps, or determinism guarantees.
- No new runtime configuration or user-facing behavior.
- No new dependencies.