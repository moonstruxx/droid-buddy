# Design: panel-rendering-fixes

## Context

See proposal.md — Why. Eight ready beads issues reported panel-rendering defects observed on `droid_mpfs5melody2.ini`, all in the controller-panel and picker/status rendering paths of `src/ui.rs` (plus LED-association detection in `src/patch.rs`). The change extends ADR 10's boxed-cell model (LED-joined cells, boxed-components-and-viewer-split) from buttons to all control kinds.

## Goals / Non-Goals

Goals:
- Every boxed cell renders complete borders at any terminal width — no partial border fragments.
- LED-joined boxes work for every control kind with a resolvable LED association.
- Deterministic, snapshot-testable rendering for all fixed behaviors (picker `..`, ellipsis, row rhythm, status dedup, scale floor).

Non-goals (see proposal): NN/ML outlier detection, latency optimizer, `.ini` mutation, hardware bridge.

## Decisions

1. **Narrow-width boxed fallback: shrink-then-fallback, never partial borders** (`src/ui.rs` render path for LED-associated cells). When the available cell width is smaller than the box content, the cell first shrinks its content to fit inside a complete box; below a floor where content no longer fits, the cell falls back to the unboxed two-line rendering. Partial border fragments are structurally impossible. Alternatives considered: always render the box and let it clip (rejected: produced the garbled cells droid_tui-wsu reported); always fall back (rejected: loses the joined-box benefit at moderate widths).

2. **Scale floor 75% instead of 50%** (`src/handler.rs` presets). Boxed cells need ~8 columns; at 50% scale (8 cols) the box content collides with the frame. 75% → 12 columns, comfortable. The floor keeps every module cell boxable at all presets. Alternatives: reworking box content to fit 8 cols (rejected: cramped, worse readability than the floor).

3. **Kind-generic joined box via per-kind (symbol, state_text, fg_color) triple** (`src/ui.rs` render_component). The boxed path already computes a per-kind state representation (knob/encoder percentage, switch glyph, button ON/OFF); extending the LED association to all element families makes the boxed path reachable for all kinds without per-kind special-casing. M hardware tokens (faders) map to `ComponentKind::Knob` (`token_kind M→Knob`) so faders flow through the same path.

4. **LED-association detection extended via shared-suffix pairing** (`src/patch.rs`). The existing bare-`led` and numbered-`ledN` suffix-pairing rule (ADR 10) generalizes from buttonN/potN to every element param family (encoderN/switchN/faderN + any schema ledN group); `led*` keys are excluded from self-pairing. The `ledN` value stays authoritative for the LED token.

5. **Picker parent entry as a `..` sentinel** (`src/app.rs` refresh_picker_entries, `src/ui.rs` render_picker). The parent-dir entry (index 0, when not at filesystem root) is marked with a sentinel so the renderer shows `..`; Enter on it navigates up without closing; entries sort directories-first then `.ini` files; no `..` at root.

6. **Uniform row rhythm from visible (LED-folded) component count** (`src/ui.rs` panel sizing). Panel height is sized from the visible component count so boxed (height 3) vs unboxed (height 2) cells no longer create irregular vertical gaps between same-kind rows.

7. **Label ellipsis preserves geometry** (`src/ui.rs`). Over-long labels truncate with a trailing `…`; cell width, alignment, and published hit rects are unchanged.

8. **Status bar segments composed once** (`src/ui.rs` render_status). Each segment (Scale, Orientation, …) is appended exactly once per frame.

## Risks / Trade-offs

- [Snapshot churn] → the joined-box and `..` behaviors are locked by new insta snapshots (visual_joined_boxes_kinds_snapshot, picker parent-entry snapshot); CI's `cargo insta test --check` is the strict gate.
- [Synthetic `[fader]` fixture sections] → DROID faders are addressed by number, not by hardware token, so the `led_pairs_kinds.ini` fader sections are synthetic; M tokens are valid graph-layer tokens used to exercise fader+LED. Documented in the fixture header.
- [.snap files are gitignored] → new snapshots must be force-added (`git add -f`) per the established convention (162 tracked snapshots).
- [Boxed fallback threshold tuning] → the exact width at which content shrinks vs falls back is a constant; if real patches expose a bad threshold, adjust the constant — the spec only requires complete borders and kind coverage, not a specific threshold.

## Migration Plan

In-place UI change, no data migration. Rollback: revert the feature commits on the branch (each task is one commit).

## Open Questions

None.