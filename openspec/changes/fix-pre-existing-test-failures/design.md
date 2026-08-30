# Design: fix-pre-existing-test-failures

## Context

See proposal.md — Why. The suite carries 16 pre-existing failures on the feature branch (641 pass), all in `regression.rs` / `rendermetrics.rs`. The main view is now the physical 1:1 rack renderer; a cluster of tests still asserts the wrapped-panel era. Two genuine renderer edge cases surface through them, plus 4 clippy lints. The verified 26q fix (stashed goal-wip) is applied to the branch, so the baseline is the issue's "16", not the clean-branch "29".

## Goals / Non-Goals

Goals:
- Green gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` all exit 0.
- Stale tests assert physical-era contracts, mirroring the passing `physical_coincidence_*` tests (DRY: reuse the same assertion style/fixtures rather than inventing new ones).
- The two renderer edge cases fixed at their source (mapping math / cell placement), not papered over in tests.

Non-Goals:
- No new capabilities, no architecture change, no wrapped-panel revival.
- No spec or CI config changes.

## Decisions

### D1: Rewrite stale tests to physical-era contracts, don't delete

The ~10 failing `regression_*` tests assert wrapped-panel behavior (boxed LED cells, uniform panel rows, `component_rects` from `render_patch_grouped`, hit rects at non-default scale). The physical renderer is the shipped main view; the tests' intent (cells render fully, no overlap, hit rects match rendered cells at any scale) still matters and is already covered by `physical_coincidence_*` for the physical view. Decision: rewrite the stale tests to drive the physical view (or the shared cell-rect invariants) using the same fixtures the coincidence tests use. Where a stale test asserts something the physical era intentionally removed (e.g. wrapped-panel row uniformity), delete the assertion, not the test's useful remainder.

Alternative considered: keep the old panel-renderer assertions against the still-existing `render_patch_grouped` code path. Rejected — that path is superseded; testing it would enshrine dead rendering.

### D2: mm→screen border overlap fix at the mapping level

The overlap (adjacent module borders share/cross a column at the current mm→chars factor ≈0.15 cols/mm) comes from rounding each module's rect independently. Decision: fix in `ScreenMapping` / the rect computation so each module's cell rect derives from the same global mm→col transform (single rounding of the shared boundary), i.e. compute column spans as `round(mm0×f)..round(mm1×f)` from absolute mm positions rather than accumulating rounded widths. This guarantees abutment at any zoom. Guard with a regression test asserting no two adjacent module rects overlap across the zoom presets.

Alternative considered: clamp rect widths post-hoc (shrink by 1 col when overlapping). Rejected — shrinks cells at all zoom levels and breaks the coincidence invariant with the skeleton.

### D3: Switch value rendering on the physical view

`S1.1` collapses onto the Pot faceplate with no switch cell. Decision: extend the physical-view element rendering to place switch cells from the controller's geometry data (same mechanism as knobs/encoders/buttons) and render the switch state (on/off glyph) on that cell, mirroring the panel view's switch rendering. Where geometry lacks a switch cell for the token's controller, omit the switch rather than mis-rendering it on a knob cell. Add a fixture asserting switch cells render and hit-test.

Alternative considered: render switches as text-only cells adjacent to the knob. Rejected — breaks the 1:1 physical-fidelity principle (ADR 15 / memory #222): switches must sit at their physical faceplate positions.

### D4: Corpus/tooling reconciliation, not artifact regeneration

`regression_scorer_holdout_agrees_with_corpus` and `rendermetrics::tests::python_rust_extractor_agreement_on_corpus` fail from corpus/extractor drift. Decision: inspect the failing assertion diffs first; re-sync the corpus files or the extractor tooling to the committed decision-table artifact (`tools/outlier_artifact.txt`, `tools/influence_stats.txt`) so the agreement holds, and only regenerate artifacts when the drift is real (scorer table genuinely out of date with the corpus). Never regenerate to force a green — the holdout gate exists to catch real model drift.

### D5: Clippy lints fixed at source

2 in `config.rs` (pre-existing), 2 in `patch.rs`. Fix with minimal, behavior-preserving edits; no `#[allow]` suppression.

## Risks / Trade-offs

- [mm→screen rounding change alters rendered geometry slightly] → The coincidence proof (`physical_coincidence_*`) and gallery snapshots re-accepted together; any face change is intentional and snapshot-verified.
- [Switch-cell placement changes existing Pot-faceplate snapshots] → Re-accept snapshots in the same wave; the `switch_value.ini` fixture asserts the new contract.
- [Corpus reconciliation accidentally masks real model drift] → Per D4, diff first; only regenerate when the scorer table is genuinely stale, and note it.
- [Test rewrites accidentally weaken coverage] → Each rewritten test keeps its original assertion intent; deleted assertions must be superseded by an existing physical-era test (verified during review).

## Migration Plan

Single-branch change (feature branch already created): implement → run the full gate → re-accept snapshots via `cargo insta accept --include-ignored` (project rule #223) → commit snapshots with `git add -f` → verify gate green again. No deployment/rollback concerns (local TUI, no data stores).

## Open Questions

None — all unknowns were resolved in exploration (failure inventory, classification, edge-case root causes).