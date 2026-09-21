# Proposal

## Why

Bead droid_tui-hud ("keybindings are not displayed with '?'") reports the help modal. The modal-open path itself is fixed at HEAD (the window key routing fix `46c6628` and the surface wiring are ancestors of the current branch; the seam test `window_question_mark_opens_help` and the paint-dispatch test `paint_dispatches_overlays_over_base` pass). What remains is the parity defect the bead calls out: the `?` modal's content, `src/help.rs::keybindings`, has drifted from the keys the handler actually binds. A user pressing `?` sees tables that omit live keys (`g s` select-state menu, `\` left-pane split, `h`/`f`/`i` graph keys, Tab focus cycling) and mislabel others (graph tension shown as `[/]` when the handler binds `Alt+[`/`Alt+]`). The help modal is only useful if the tables describe the real surface.

## What Changes

- **`src/help.rs`**: sync every `keybindings(HelpView)` table to the handler's current bindings.
  - Panels: add `\` (toggle left-pane vertical split), `g s` (open select-state menu), `Tab`/`Shift+Tab` (cycle pane focus).
  - Graph: add `h` (toggle column/force layout), `f` (dependency filter), `i` (influence filter), `g s` (select-state menu); correct cable-tension rows from `[/]` to `Alt+[`/`Alt+]`.
  - Physical: fill the near-empty table with `+/-` (zoom presets), arrows (pan rack), `j`/`k` (navigate), `s` (skeleton toggle, already present).
  - Viewer / Validation / Optimizer / Picker: verify against the handler and correct any drift found.
- **Tests**: extend the `help.rs` table tests so each view's rows reflect the handler's live bindings (the existing `keybindings_reflect_graph_center_fit_and_g_c_chord` and `keybindings_include_carousel_key` tests set the pattern), and add a handler-side parity test pinning `?` + the added keys.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `keybinding`: the `?` help modal's content must stay in parity with the handler's live bindings for every view (Panels, Viewer, Graph, Physical, Validation, Optimizer, Picker).

## Impact

- Affected code: `src/help.rs`, `src/handler.rs` (tests), possibly `src/gui/mod.rs` (only if a surface paints keys the table lacks).
- Affected specs: `keybinding` (delta).
- Baseline: full suite stays green; `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`.

## Non-goals

- No change to any handler keybinding itself.
- No rework of the help modal's rendering or layout (it already paints correctly).
- No keybinding reconfiguration or customization.