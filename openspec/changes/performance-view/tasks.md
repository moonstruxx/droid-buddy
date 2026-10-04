# Tasks

## 1. Performance view core

- [x] 1.1 View state + `p` routing + `Esc` exit (App flag, Physical-pane `p` arm opening the view with status, `Esc` leaving it; other panes' `p` untouched) and verify the routing test passes <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs, src/handler.rs] -->
- [x] 1.2 Side-bound label placement model (pure `src/performance.rs`: quadrant side assignment, within-side ordering, stacking, leader anchors; no egui dependency) and verify the placement tests pass <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/performance.rs] -->
- [x] 1.3 Paint the performance view (compact centered rack + callouts + leader lines via one new theme token across palettes, guessed-first content) and verify the paint tests pass <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [src/gui/physical.rs, src/theme.rs] -->
- [x] 1.4 Element-state reset key + live callout state (reset-all restoring rest state only, selection/focus/shift untouched) and verify the reset test passes <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs, src/handler.rs] -->

## 2. Docs

- [x] 2.1 Add the module-UI help-table rows (`p`, reset) and verify the help test passes <!-- agent: rusty-engineer.fast, depends_on: [1.1, 1.4], touches: [src/help.rs] -->

## 3. Verification

- [ ] 3.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) and verify all four exit 0 <!-- agent: horst-engineer.fast, depends_on: [1.1, 1.2, 1.3, 1.4, 2.1], touches: [] -->
