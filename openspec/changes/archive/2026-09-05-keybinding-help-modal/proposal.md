# Change: keybinding-help-modal

## Why

The app has grown many surfaces (panels/physical view, embedded source viewer, signal-flow graph, quad view, validation modal, optimizer menu, file picker), each with its own keybindings, and there is no way to discover them from inside the TUI. A `?` key that opens a floating help modal listing the current view's bindings closes that gap without moving documentation outside the app.

## What Changes

- **`?` opens a floating help modal** (`src/handler.rs`): handled at the top of the priority chain, below the label-edit overlay, so it works from every view. While the modal is open it eats all keys except `Esc` and `q`, both of which close it; `q` does not quit while the modal is open.
- **View-dependent content** (`src/help.rs`, new pure module): a `HelpView` enum, `active_view(&App)` mirroring the handler priority chain, and `keybindings(HelpView)` returning the binding list for the active view (panels/physical, viewer, graph, quad, validation, optimizer, picker). No terminal dependency, matching `diff.rs`/`graph.rs`/`validation.rs`.
- **Floating modal render** (`src/ui.rs`): a centered 60% x 70% popup listing key/description rows, publishing `help_modal_rect` per frame for click-outside hit-testing; rendered as the top z-layer so it can overlay the picker.
- **Click-outside close** (`src/handler.rs`): a left-button Down outside the modal rect closes it, checked before the quad/graph mouse branches so it works over any surface.
- **Theme tokens** (`src/theme.rs`): `help_modal_border` and `help_modal_selected_bg` per palette, following the `optimizer_modal_*` precedent.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `keybinding`: adds the `?` help-modal requirement (open from any view, view-dependent content, close via `Esc`/`q`/click-outside).

## Impact

- Affected specs: `keybinding` (delta)
- Affected code: `src/help.rs` (new), `src/lib.rs`, `src/app.rs`, `src/handler.rs`, `src/ui.rs`, `src/theme.rs`, `src/regression.rs`, `fixtures/*`
- Baseline: full suite stays green; `cargo insta test --check` remains the strict gate.

## Non-goals

- No keybinding reconfiguration or user customization
- No search or filter inside the help modal
- No change to any existing keybinding's behavior outside the modal's lifetime