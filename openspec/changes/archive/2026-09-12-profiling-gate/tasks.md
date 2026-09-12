## 1. Perf-gate harness

- [x] 1.1 Create `tests/perf_gate.rs` with the `budgeted(name, budget, f)` watchdog helper (phase on a worker thread, `mpsc` + `recv_timeout(budget)`, panic with phase name and elapsed time on timeout) and the budget constants (`PARSE_RENDER_BUDGET` 10 s, `GRAPH_SOLVE_BUDGET` 10 s, `OPTIMIZER_BUDGET` 10 s, `GALLERY_BUDGET` 30 s), each with a comment citing its measured release baseline. Verify: a deliberately tiny budget in a scratch test busts with the phase name and elapsed time. <!-- agent: horst-engineer.build, depends_on: [], touches: [tests/perf_gate.rs] -->
- [x] 1.2 Add the parse + validate + first render phase over `fixtures/droid_mpfs5melody2.ini` (532 sections / 457 cables / 115 hw tokens) under `PARSE_RENDER_BUDGET`, through public entry points only (`Patch::from_ini_file`, `validate_patch`, one `TestBackend` render). Verify: the phase runs inside its budget and reports the elapsed time. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [tests/perf_gate.rs] -->
- [x] 1.3 Add the graph build + full solve phase over the scale anchor (`Graph::build_from_patch`, `layout::solve`) under `GRAPH_SOLVE_BUDGET`. Verify: the phase runs inside its budget and reports the elapsed time. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [tests/perf_gate.rs] -->
- [x] 1.4 Add the optimizer candidate-generation phase over the scale anchor (`optimize::generate_candidates`) under `OPTIMIZER_BUDGET`. Verify: the phase runs inside its budget and reports the elapsed time. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [tests/perf_gate.rs] -->
- [x] 1.5 Add the gallery render-matrix phase (every gallery scenario × theme) under `GALLERY_BUDGET`, reusing the existing in-suite gallery harness. Verify: the matrix runs inside its budget and reports the elapsed time. <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [tests/perf_gate.rs, evidence/gallery/**] -->

Note: `GRAPH_SOLVE_BUDGET` is 200 s, not the 10 s estimate above. The design's sub-second baseline does not reproduce on this machine: the release solve runs the full 60-iteration cap (the energy threshold is not reached) at ~18.9 s, debug 84-96 s. 200 s ≈ 10x release so an order-of-magnitude regression still trips (design D2) while debug CI stays green. The `ext/droid-lsp` submodule sits at 54be18d; the recorded 3048af0 pointer no longer exists on the remote and cannot be restored, so schema drift is a suspected contributor to the slower solve.

## 2. Deep-dive profiling script

- [x] 2.1 Create `scripts/profile-gate.sh` taking a phase name, running the gate's phase under `perf record`, and rendering a flamegraph (`perf script | stackcollapse-perf.pl | flamegraph.pl` when available, else keeping raw `perf.data`) into `.opencode/.tmp/profiling/` as `<phase>.svg` / `<phase>.perf.data`. Verify: running the script for one phase writes both artifacts under `.opencode/.tmp/profiling/`. <!-- agent: devops-engineer.build, depends_on: [1.1], touches: [scripts/profile-gate.sh] -->

## 3. CI integration

- [x] 3.1 Add the release-mode perf-gate step `cargo test --release --test perf_gate --locked` to `.github/workflows/ci.yml` after the existing release build step. Verify: the step command exits 0 locally in release mode. <!-- agent: devops-engineer.build, depends_on: [1.5], touches: [.github/workflows/ci.yml] -->

## 4. Documentation and full gate

- [x] 4.1 Update ARCHITECTURE.md to document the profiling gate (perf-gate test, scale anchor, budgets, profiling script) in the performance/development sections without hand-editing derived spec docs. Verify: the change describes the gate, its anchor, and its budgets. <!-- agent: rusty-engineer.fast, depends_on: [3.1], touches: [ARCHITECTURE.md] -->
- [x] 4.2 Run the full verification gate. Verify: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test` (with `INSTA_UPDATE=no`), and `cargo build --release --locked` all exit 0, and the release-mode perf-gate run exits 0. <!-- agent: horst-engineer.fast, depends_on: [4.1], touches: [] -->