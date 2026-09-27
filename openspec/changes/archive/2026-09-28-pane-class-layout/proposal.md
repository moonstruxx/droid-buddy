# Proposal

## Why

The latency optimizer paints as an overlay card on top of the module UI pane instead of occupying a pane of its own, so opening it hides the panels it is meant to be compared against. The `optimizer-pane` spec already requires a side pane and the code never implemented it: `paint_tiled` leaves the `ViewType::Optimizer` slot empty and `paint_overlays` draws the card over the panels.

The layout model is being replaced at the same time. The current tiling (a permanent left panels pane plus a right column of up to three stacked slots) and the 2x2 quad view are both retired in favour of a class-based layout: one big pane and two small panes, two window classes, a non-latching maximize, and same-class swap.

## What Changes

- **BREAKING**: the tiling layout (left panels pane plus a right column of up to three slots) and the 2x2 quad view are removed. The `tiling-window-manager` and `quad-view` capabilities are retired.
- New class-based layout: three panes in a fixed tree. The big pane takes the left half at full height, and the right half splits vertically into two small panes of a quarter each.
- Window classes: Big covers the signal-flow graph, the module UI (panels), and the physical rack view. Small covers the optimizer and the source viewer.
- Panels become a view. The permanently visible left pane goes away and the module UI occupies the big pane.
- Startup layout: module UI in the big pane, source viewer in a small pane.
- The optimizer opens in a small pane. The overlay card path is deleted, which closes the gap against `optimizer-pane`.
- Maximize: `z` toggles the focused pane to the full main band. It does not latch, so pressing `z` again, moving focus, or pressing `Esc` restores the split.
- Swap: `Alt+b` exchanges the big pane's view with the focused pane's view, which promotes a small-pane view into the big slot. `Alt+s` exchanges the two small panes.
- The help tables and the `keybinding` spec gain `z`, `Alt+b`, and `Alt+s`.

## Capabilities

### New Capabilities
- `pane-class-layout`: the class-based three-pane layout, its geometry, class routing, focus cycling, maximize, and same-class swap.

### Modified Capabilities
- `optimizer-pane`: the optimizer must occupy a pane rather than an overlay card, and its `[`/`]` weight binding takes precedence over the pane boundary adjustment while it is focused.
- `keybinding`: `z`, `Alt+b`, and `Alt+s` are added, and the help modal tables must list them.

### Removed Capabilities
- `tiling-window-manager`: superseded by `pane-class-layout`.
- `quad-view`: superseded by `pane-class-layout`.

## Impact

- Affected code: new `src/panes.rs`; `src/app.rs` (layout state, geometry, open and close, focus), `src/handler.rs` (keys and pane routing), `src/gui/mod.rs` (paint dispatch), `src/gui/overlays.rs` (drop the optimizer overlay), `src/gui/panels.rs` (panels as a pane view), `src/help.rs` (key tables), `src/lib.rs` (module wiring).
- Affected specs: `pane-class-layout` (new), `optimizer-pane` and `keybinding` (modified), `tiling-window-manager` and `quad-view` (removed).
- Working-tree risk: `src/gui/mod.rs` and `src/gui/overlays.rs` carry about 490 uncommitted lines from the archived `help-keybinding-parity` change on branch `feature/help-keybinding-parity`. That work must be committed or stashed before implementation starts, or the two edit sets will collide.
- Verification: the four project gates, `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, and `cargo build --release --locked`.

## Non-goals

- No change to the graph, panel, picker, or validation internals beyond the pane they are drawn into.
- No persistence of the layout across restarts.
- No pointer drag to resize panes. The ratios stay on the keyboard.
- No new view types beyond treating panels as one.
- No change to the key sets inside the source viewer, the optimizer, or the graph, apart from the new layout keys.
