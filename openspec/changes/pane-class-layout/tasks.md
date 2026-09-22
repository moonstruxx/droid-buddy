# Tasks

## 1. Pane layout model

- [x] 1.1 Add `PaneClass`, `PaneId`, `Pane`, and `PaneLayout` in a new `src/panes.rs`, add `ViewType::Panels`, and add `ViewType::class()`; verify with `cargo build --lib` and a unit test asserting each view's class <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/panes.rs, src/lib.rs] -->
- [x] 1.2 Replace `TileStack` in `App` with `PaneLayout`, including defaults, the startup configuration (module UI in the left big pane, source viewer in a small pane), class-routed open and close, the physical and module UI exclusivity rule, and the arrangement transitions when the first small view opens or the last one closes; verify with `cargo test --lib app` <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [x] 1.3 Add the pane geometry for both arrangements (left big pane at the left and right ratio, right half as one big pane when no small view is open, right half split vertically at the small ratio when one is, plus the maximize override) and publish `pane_rects`; verify with unit tests over the default and clamped ratios in both arrangements <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/app.rs] -->
- [x] 1.4 Implement `maximize_toggle`, `swap_big`, `swap_small`, and pane focus cycling, including the no-op cases (only one big pane, no small panes) and the clears on `Esc` and on focus change; verify with unit tests <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/app.rs] -->

## 2. Paint the class layout

- [ ] 2.1 Replace `paint_tiled` with `paint_panes`, painting both arrangements, the maximize override, the focus borders, and each pane's view; verify with headless egui shape tests <!-- agent: layout-designer-engineer.build, depends_on: [1.4], touches: [src/gui/mod.rs] -->
- [ ] 2.2 Remove the optimizer overlay path from `paint_overlays` and paint the optimizer inside its small pane by passing the pane rect to `paint_optimizer`; verify that no overlay card rect is emitted while the optimizer is open <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/mod.rs, src/gui/overlays.rs] -->
- [ ] 2.3 Make the panels paintable as a pane view and drop the always-on left pane assumption; verify with headless shape tests <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/panels.rs, src/gui/mod.rs] -->

## 3. Keys and handler routing

- [ ] 3.1 Route `z`, `Alt+b`, and `Alt+s` in `handle_event` under the existing focus, overlay, and picker priority, and make `Esc` clear the maximize before closing a view; verify with handler tests <!-- agent: rusty-engineer.build, depends_on: [1.4], touches: [src/handler.rs] -->
- [ ] 3.2 Route pane keys by the focused pane's view (graph, viewer, optimizer, panels) and keep `[`/`]` view-local while the optimizer is focused; verify with handler tests <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs] -->
- [ ] 3.3 Add `z`, `Alt+b`, and `Alt+s` and the pane model to the `src/help.rs` tables; verify with `cargo test --lib help` <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/help.rs] -->

## 4. Tests

- [ ] 4.1 Pane model tests: geometry in both arrangements, class routing, replace-on-open, physical and module UI exclusivity, the arrangement transitions on first open and last close, maximize toggle and clears, swap big and small including both no-op cases, and focus cycling with an empty pane; verify with `cargo test --lib panes` <!-- agent: horst-engineer.build, depends_on: [1.4], touches: [src/panes.rs] -->
- [ ] 4.2 Headless egui shape tests for `paint_panes`: both arrangements, maximize filling the band, focus border tokens, and the optimizer drawn inside its pane with no overlay card; verify with `cargo test --lib gui` <!-- agent: horst-engineer.build, depends_on: [2.2, 2.3], touches: [src/gui/mod.rs] -->
- [ ] 4.3 Handler parity tests for `z`, `Alt+b`, and `Alt+s` dispatch and for the help tables listing them; verify with `cargo test --lib handler help` <!-- agent: horst-engineer.build, depends_on: [3.3], touches: [src/handler.rs, src/help.rs] -->

## 5. Spec sync and verification gate

- [ ] 5.1 Sync the spec deltas and remove the `tiling-window-manager` and `quad-view` specs, and flag `ARCHITECTURE.md` and `DESIGN.md` for regeneration rather than hand-editing them; verify with `openspec validate pane-class-layout --strict` <!-- agent: devops-engineer.fast, depends_on: [4.3], touches: [openspec/specs/**] -->
- [ ] 5.2 Run the full verification gate and confirm all four exit 0: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` <!-- agent: rusty-engineer.fast, depends_on: [5.1], touches: [] -->
