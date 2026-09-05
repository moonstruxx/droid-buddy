# Handoff: optimizer budget tests + layout settle tests (2026-09-02)

## 1. What I was working on

Adding performance/regression tests that lock in two properties: the layout solver settles quickly on real patches, and the latency optimizer respects its step budget. Both test sets are uncommitted.

**`src/layout.rs`** (new, +59 lines):
- `graph_from_fixture(name)` helper: loads a fixture `.ini`, builds the graph.
- `real_patch_solve_settles_quickly`: full `solve()` on `fixtures/arpeggio1.ini`, asserts the iteration count stays under a bound.
- `real_patch_local_resettle_settles_quickly`: `local_resettle` on the same patch, asserts a bounded iteration count.

**`src/optimize.rs`** (new, +183/-8):
- `EvalCtx.scored: Cell<usize>` counter, incremented in `evaluate_order` (test observability; was already in the working tree from a previous session).
- `generate_candidates_scored_inner`: returns `(candidates, scored)` alongside the existing budgeted generator.
- `search_respects_step_budget`: each heuristic search (local/VNS/SA) scores at most `budget + 2` on a synthetic `big_patch`.
- `generate_candidates_stays_within_budget`: full and fast candidate generation stay within budget on `big_patch` and `large_banner_patch`.
- `real_patch_optimizer_completes_within_budget`: candidate generation with `SEARCH_STEPS = 2000` on `droid_mpfs5drum` and `droid_mpfs5melody2`.

**Parallel-test-timing investigation**: `cargo test real_patch` (the filter matches all three real-patch tests) ran past 60 s and was killed. I read all three search functions and confirmed each is budget-bounded (every candidate evaluation decrements the budget, loops exit at 0), so the slowness is not an unbounded search loop. The actual slow path was not yet identified when this handoff was requested.

## 2. Current state of uncommitted changes

- `M src/layout.rs`: helper + 2 tests. `real_patch_local_resettle_settles_quickly` passes; `real_patch_solve_settles_quickly` fails (real solver exceeds the asserted iteration bound, needs calibration).
- `M src/optimize.rs`: counter + scored generator + 3 tests. The two synthetic tests pass; `real_patch_optimizer_completes_within_budget` is too slow (killed after 60 s+).
- `? ext/droid-lsp`: the submodule shows untracked content. This is the temporary symlink `ext/droid-lsp/src/circuits.json -> ../droid-lsp/src/circuits.json` created per constraint #195 (schema.rs does `include_str!("../ext/droid-lsp/src/circuits.json")`, which does not exist in the vendored submodule layout). It is required for `cargo clippy`/`cargo test` to compile. It lives inside the submodule working tree, so it is not tracked by the main repo and cannot be accidentally committed. Leave it in place while the gate needs to run; remove it only if the schema path is fixed at repo level.
- `.opencode/` untracked files are pre-existing orchestration config, not part of this work.
- The earlier graph-view fix (`src/graph_render.rs` fit_to_world pan-anchor + `src/ui.rs` render tests) is committed; it no longer appears in `git status`.

## 3. Unresolved

- `real_patch_solve_settles_quickly` fails: the real solver on arpeggio1.ini runs more iterations than the asserted bound. Either the bound needs calibration to measured behavior, or the solver genuinely does not settle as fast as assumed.
- `real_patch_optimizer_completes_within_budget` is too slow. All search loops are budget-bounded, so the cost sits elsewhere: the `search_exact`/`combine_rec` path (if `total_valid <= ENUM_LIMIT` for melody2's banner domains), the cost of `derive()`/`new_eval_ctx`, or `evaluate_order` being O(n + edges) across ~6000 evaluations on the 379-section melody2.
- Open question: is the slow test a test-design problem (budget too generous, real patches too big) or a real optimizer performance problem?

## 4. Concrete next steps

1. Find the slow path in `real_patch_optimizer_completes_within_budget`. Print `eval.scored.get()` per strategy, or time `derive`/`new_eval_ctx`/`search_exact` separately. Check whether melody2's BannerMinSum domains make `total_valid <= ENUM_LIMIT`, which routes to `search_exact`/`combine_rec` (bounded at 50000 evaluations but each is O(n + edges)).
2. Fix or redesign the slow test: reduce the budget for real patches (e.g. `INTERACTIVE_SEARCH_STEPS`), use a smaller real fixture, or assert on the `scored` counter instead of wall-clock time (the counter exists for exactly this).
3. Calibrate `real_patch_solve_settles_quickly`: measure the actual iteration count on arpeggio1.ini and set the bound with headroom.
4. Run the four-gate suite: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`.
5. Keep the `ext/droid-lsp/src/circuits.json` symlink until the schema path issue is fixed at repo level; it is not tracked and cannot be committed.