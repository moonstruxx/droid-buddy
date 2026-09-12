# Design: profiling gate

## Context

See proposal.md (Why). Measured baselines (release build, this worktree, 2026-09-12): the full gallery matrix renders in ~0.32 s CPU; melody2 (532 sections / 457 cables / 115 hw tokens) parses, validates, builds, solves, and renders in well under a second; the solver is bounded at 60 iterations and the optimizer at its search budget. The gate must turn a regression in any of these phases into a named, bounded failure instead of a hung test suite.

## Goals / Non-Goals

Goals:
- A default-suite test that fails with the phase name and elapsed time when a gated phase exceeds its budget.
- Budgets robust enough to stay green on slow machines and in debug builds, tight enough to trip on genuine hogs and loops.
- A release-mode CI run so budgets measure honest release timings.
- A profiling script so a busted budget produces flamegraph evidence.

Non-Goals:
- No benchmark harness, no tight thresholds, no per-commit performance tracking.
- No changes to the solver, optimizer, parser, or renderer algorithms.

## Decisions

### D1: Watchdog helper runs each phase on its own thread with a recv_timeout ceiling

An in-process `Instant::elapsed` check after the phase returns cannot catch an unbounded loop, because the loop never returns. Spawning the phase on a worker thread and waiting on a `mpsc` channel with `recv_timeout(budget)` turns a hung phase into a `panic!` with the phase name and elapsed time, while the suite continues to a red result. The leaked worker thread dies with the test process. The helper signature is `budgeted(name, budget, f)`.

Alternative considered: `std::time::Instant` around an inline call, simpler but blind to loops. Rejected: the whole point is busting endless loops.

### D2: Budgets are named constants, release-anchored, generous

Each budget documents its measured release baseline in a comment and multiplies by a wide margin (phases 10 s, matrix 30 s, vs sub-second and ~0.32 s baselines). Generous margins keep the gate green under debug builds (5–10x slower) and slow CI runners, while an order-of-magnitude regression or a loop still trips it. The gate runs in the default `cargo test` suite (not `#[ignore]`), so a hog busts everywhere, and CI additionally runs it in release for honest timings.

### D3: Integration test in `tests/perf_gate.rs`, public entry points only

The gate lives in `tests/` and drives existing public APIs (`Patch::from_ini_file`, `validate_patch`, `Graph::build_from_patch`, `layout::solve`, `optimize::generate_candidates`, and the gallery harness). No `src/` code changes: the gate measures the crate as a consumer sees it, and it cannot accidentally relax internal caps.

Alternative considered: an in-crate `#[cfg(test)]` module, which could call private internals but risks coupling to internals and being skipped by the default suite. Rejected: public-only keeps the gate honest and simple.

### D4: Scale anchor is the tracked melody2 fixture

`fixtures/droid_mpfs5melody2.ini` (532 sections / 457 cables / 115 hw tokens) is the largest real patch in the corpus and the measured worst case. It is already tracked, so the gate needs no new fixture. The gallery matrix phase reuses the existing in-suite gallery harness, so the render phase needs no new render code either.

### D5: Deep-dive script writes evidence under `.opencode/.tmp/profiling/`

`scripts/profile-gate.sh` takes a phase name, runs the gate's phase under `perf record`, and renders a flamegraph (`perf script | stackcollapse-perf.pl | flamegraph.pl` when available, else keeps the raw `perf.data`). Output lands under `.opencode/.tmp/profiling/` (repo scratch convention, gitignored), naming files `<phase>.svg` / `<phase>.perf.data` so a busted budget has reproducible evidence.

### D6: CI runs the gate in release mode

`cargo test --release --test perf_gate --locked` is added to `.github/workflows/ci.yml` after the existing release build step, so the gate busts on release timings in CI. Debug-mode `cargo test` already runs the same gate in the default suite; generous budgets keep both green.

## Risks / Trade-offs

- [Budget too tight on a slow CI runner] → Margins are 30–100x the measured baseline; only an order-of-magnitude regression or a loop trips them.
- [Budget too loose to catch a moderate regression] → The gate targets hogs and loops, not benchmarks; a 2x slowdown stays green by design (non-goal).
- [Leaked worker thread after a timeout] → The test process exits and reaps it; the suite reports the failure and continues. No state is shared with the worker after the panic.
- [Flamegraph tools not installed] → The script degrades to keeping raw `perf.data` and prints the install hint.
- [melody2 fixture changes shape later] → Budgets are generous enough to absorb fixture growth; the scale anchor is the largest real patch today.

## Migration Plan

Additive: new test file, new script, one CI step. No runtime behavior changes, no rollback surface beyond removing the CI step and the test file.

## Open Questions

None.