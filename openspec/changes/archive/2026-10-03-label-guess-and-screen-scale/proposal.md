# Proposal

## Why

Labels already exist everywhere the user asked for them: the module UI draws an ellipsized label per referenced element (`module-ui` shift-aware labels), the graph draws a node title (`circuit_display_label`), the `e` overlay edits them, and `labels.toml` stores them locally without mutating the patch file. Two gaps surfaced in exploration:

1. **Fallback labels are meaningless.** A circuit with no stored label falls back to its raw circuit name (`pulser`, `envelope`). The user wants a guessing function that walks up the signal-flow tree and takes the first port or circuit label it finds.
2. **Labels are coupled to zoom.** Graph labels shrink with the camera and disappear below a legibility threshold; module-UI labels scale with cell height. The user wants labels at their own size level: a fixed screen-space font (option B) with the fit-or-show policy B3 — fixed size, ellipsized to the frame, hidden below about one character, with the existing hover tooltip/status bar carrying the full label.

## What Changes

- **Derived circuit labels**: the circuit display chain becomes stored label → tree-derived label → raw circuit name. The derivation walks the graph upstream from the circuit (root excluded) in deterministic breadth-first order and returns the first *explicit* label found — an upstream circuit's stored label, or an upstream controller/jack (port) token's stored-or-preamble label. Derived names (`Button B3.17`, raw circuit names) never count as guesses.
- **Screen-space labels**: graph node titles and module-UI cell labels render at a fixed font size independent of camera/physical zoom; text wider than the frame is ellipsized; below about one character of width the label is hidden. Port markers, cluster titles, and all chrome keep their existing zoom-proportional behavior.
- **Verification only** for "labels in the physical view and the graph view": both surfaces already render labels by spec; the change proves it end-to-end at the new sizing rules.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `label-management`: ADDED requirement *Derived circuit labels from the signal-flow tree* (explicit-only, nearest-first, deterministic; no graph built → no guess).
- `graph-node-scaling`: MODIFIED *Level of detail at extreme zoom* (labels no longer shrink; fixed screen-space font with ellipsize-to-frame and one-character hide; port markers/cluster titles keep their threshold).
- `physical-scale-model`: MODIFIED *Physical cell rendering contract (compact-only)* (label at fixed screen-space size, omitted below one character of cell width).

## Non-goals

- No global/cross-patch label store — per-patch isolation stays; "independent of the patch" means content-independent (no `.ini` mutation), which is already true.
- Guesses are not persisted — computed from the graph at (re)build time and refreshed on label save; only user edits are stored.
- No cable/edge labels.
- No `[labels] size` config knob and no new keybindings — one base label size per surface.
- No change to the editing overlay UX or to the hardware label chain (store → preamble → derived).
- Cluster-title and port-marker sizing are unchanged.
- `ARCHITECTURE.md` / `DESIGN.md` are derived artifacts and are regenerated at archive, not hand-edited.

## Impact

**Affected code:** `src/graph.rs` (upstream-label derivation), `src/app.rs` (guess cache), `src/gui/graph.rs` + `src/gui/viewer.rs` (resolution wiring), `src/gui/graph.rs` + `src/gui/physical.rs` (screen-space label sizing), `src/regression.rs` (cross-layer coverage).

**Tests:** new unit tests for the derivation walk and the cache lifecycle; headless egui shape/label tests asserting a constant font size across zoom presets, ellipsis at medium frames, and hiding at one-character frames on both surfaces; a cross-layer regression story over the scale anchor; live-screen proof per the visual-validation rule.

**No config, dependency, data-format, or keybinding changes.**
