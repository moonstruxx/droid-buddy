# Design — 2026-08-29-nn-ui-render-outlier-detection

## D1 — Distilled decision table, embedded via `include_str!` (Track-2 precedent)

The runtime model is a compact bounded decision table (≤ a few KB) fit offline and embedded in the binary with `include_str!`, exactly like the rack-wiring-outlier scorer's artifact. No ML runtime, no new dependencies, no network. The offline fit script and the Rust scorer are the only two consumers of the artifact's schema; a schema drift between them breaks a test, not a user session.

**Rationale:** the Track-2 pattern already proved this shape (bounded table, deterministic, zero-dependency); the roadmap's signal-2 note names it as the established convention. YAGNI: a table is enough to separate the observed degraded/healthy bands; a general model is speculative.

## D2 — Extractor computes from layout math, not a rendered frame

`src/rendermetrics.rs` derives features from the renderer's own constants (`COMPONENT_WIDTH/HEIGHT`, scale presets, sidebar/minimap thresholds, boxed-cell fallback rule, theme token values) — pure functions over `(Patch, width, theme)`. No `TestBackend` render is needed at runtime: the features are exactly the quantities the renderer's layout decisions depend on, so the extractor and the renderer cannot drift (both read the same constants).

**Rationale:** runtime-cheap (no double render), deterministic (D9-compatible), and testable without a terminal. The rendered-frame path is only used by the gallery harness for proof, not by the extractor.

## D3 — Corpus: widths × themes, known-good + injected known-bad, SEED 42

The offline corpus covers the existing fixture/corpus patches × widths (80/100/120) × themes (classic/mono/terminal). Labels come from two sources: known-good renders (patches at widths where the renderer produces no fallback/clipping) and injected known-bad renders (narrow widths that force boxed→unboxed fallback, oversized patches at 80 cols). Generation is deterministic with `random.Random(SEED=42)`, mirroring `tools/build_features.py`'s determinism contract.

**Rationale:** the label semantics must be auditable (Track-2 1.1 lesson: imbalanced/leaked labels silently overfit the generator). Injected bads guarantee the table sees both classes across every feature axis.

## D4 — Python tool replicates the Rust extractor (build_features.py precedent)

`tools/build_rendermetrics.py` re-implements the feature extraction in stdlib Python for offline fitting, with a regression check that the Python and Rust extractors agree on the corpus (a test compares a sampled feature vector). The fit script (`tools/fit_render_model.py`) emits the artifact consumed by `include_str!` and prints a holdout precision/recall report vs the heuristic baseline, gated (precision ≥ 0.60 at recall ≥ 0.86 on holdout, mirroring the Track-2 gate).

**Rationale:** the Track-2 tooling already established this two-language replica pattern; keeping the Python side stdlib-only preserves the no-toolchain-offline invariant.

## D5 — Invariant guards and non-blocking surface

The scorer enforces three invariants regardless of table content: (1) never flag a render at or above the patch's native-fit width; (2) never flag when the heuristic baseline is clean; (3) a table miss falls back to the baseline rule. The runtime surface is a status hint in a `render_outlier_warning` token — it never gates `load_patch`, never blocks rendering, and never intercepts input.

**Rationale:** the warning is advisory (like topology findings); gating on it would punish users for terminal size. The guards mirror Track-2's invariant-guard design (adjacent/co-located never flagged, miss → fallback) so the learned table can only add warnings, never remove the baseline's guarantees.

## Schema of the distilled artifact

```json
{
  "version": 1,
  "feature_names": ["components", "panels", "modules", "min_width", "overflow_cols", "fallback_rate", "sidebar_hidden", "minimap_hidden", "min_contrast"],
  "degraded_bands": [ {"min": 0, "max": 0.1, "degraded": false}, ... ],
  "baseline_min_width_factor": 1.0,
  "seed": 42
}
```

## Proof strategy

- Extractor unit tests (feature values for known fixtures at known widths).
- Scorer tests: scored-outlier case, fallback case, native-fit-never-flagged, baseline-clean-never-flagged.
- Python↔Rust extractor agreement on a sampled corpus.
- Holdout precision/recall regression asserted from tooling output in a test (Track-2 3.1 precedent).
- Snapshot matrix: the status-hint channel renders in classic/mono/terminal at widths 80/100/120.
- Gallery-CI flag: predicted-bad scenarios are marked in the gallery output.