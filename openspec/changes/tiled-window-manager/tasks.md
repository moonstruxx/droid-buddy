## 1. Tiling State Model

- [x] 1.1 Add `TileStack`, `ViewType`, and `FocusSlot` types to `app.rs`; replace `showing_viewer`, `showing_graph`, `showing_quad`, `showing_optimizer` with `tile_stack: TileStack`; rename `viewer_split_ratio` to `main_split_ratio`; add `left_split_ratio` for vertical split toggle. Verify `cargo clippy --all-targets --all-features -- -D warnings` passes. <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [x] 1.2 Add `open_view(ViewType)`, `close_focused_view()`, `cycle_focus()`, `cycle_view_in_slot()`, `toggle_left_split()` methods to `App`; wire `recompute_influence` for modifier selection within the tiled model. Add unit tests for state transitions (open, close, cycle, split toggle). Verify `cargo test` passes. <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->

## 2. Theme Tokens

- [x] 2.1 Add `pane_focus_border` and `pane_unfocused_border` theme tokens to `theme.rs` for `classic`, `mono`, and `terminal` palettes. Remove `optimizer_modal_border` (replaced by pane tokens). Verify `cargo clippy --all-targets --all-features -- -D warnings` passes. <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/theme.rs] -->

## 3. Tiling Renderer

- [x] 3.1 Implement `render_tiled_main` in `ui.rs`: left pane (panels) + right column (horizontal cuts for open views). Publish `pane_rects` per frame for hit-testing. Render focus borders using new theme tokens. Verify `cargo clippy --all-targets --all-features -- -D warnings` passes. <!-- agent: layout-designer-engineer.build, depends_on: [1.1, 2.1], touches: [src/ui.rs] -->
- [ ] 3.2 Implement narrow-terminal fallback: below 120 cols, collapse right column and show "+N views hidden" status hint. Verify snapshot at 80 cols shows single-pane layout. <!-- agent: layout-designer-engineer.build, depends_on: [3.1], touches: [src/ui.rs] -->

## 4. Keybinding and Focus Routing

- [ ] 4.1 Rewrite `handle_event` key priority chain: focused pane dispatch replaces per-view flags. `Tab` cycles focus across panes; `Shift+Tab` cycles backward. `Esc` closes focused view or clears modifier selection. Verify `cargo test` passes with updated handler tests. <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/handler.rs] -->
- [ ] 4.2 Consolidate zoom family: `+`/`-` scale focused pane, `Shift++`/`Shift+-` scale other pane, `[`/`]` adjust `main_split_ratio`, `Alt+[`/`Alt+]` adjust cable tension (graph-focused), `\` toggles left vertical split. Verify keybinding tests pass. <!-- agent: rusty-engineer.build, depends_on: [4.1], touches: [src/handler.rs] -->

## 5. Optimizer Pane

- [ ] 5.1 Promote optimizer from modal to side pane: replace `render_optimizer_modal` with pane renderer in right column. `g o` opens optimizer as a view slot. Remove modal overlay path. Verify `cargo test` passes and optimizer renders in tiled layout. <!-- agent: rusty-engineer.build, depends_on: [1.1, 3.1], touches: [src/app.rs, src/ui.rs, src/handler.rs] -->

## 6. Quad View Removal

- [ ] 6.1 Remove `showing_quad`, `quad_focus`, `filtered_graph`, `filtered_positions` from `App`; remove `g q` binding; remove `render_quad` path. Vertical split (`\`) survives as left-pane sub-split. Verify `cargo clippy --all-targets --all-features -- -D warnings` passes and no dead code remains. <!-- agent: rusty-engineer.build, depends_on: [1.1, 3.1, 4.2, 5.1], touches: [src/app.rs, src/handler.rs, src/ui.rs] -->

## 7. Tests

- [ ] 7.1 Unit tests: `TileStack` state transitions (open/close/cycle/split), focus routing (Tab cycle, mouse click), carousel rotation, narrow-terminal collapse. Verify `cargo test` passes with new test coverage. <!-- agent: horst-engineer.build, depends_on: [1.2, 4.2, 3.2, 6.1], touches: [src/app.rs, src/handler.rs, src/ui.rs, fixtures/] -->
- [ ] 7.2 Snapshot + gallery matrix: tiled layouts (1/2/3 views) × themes × widths, focus states, narrow collapse, optimizer pane. Update existing snapshots for new layout. Verify `cargo insta test --check` passes. <!-- agent: horst-engineer.build, depends_on: [3.1, 3.2, 5.1, 6.1], touches: [src/snapshots/**, evidence/gallery/**] -->

## 8. Verification Gate

- [ ] 8.1 Run full verification: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`. All four must exit 0. <!-- agent: rusty-engineer.fast, depends_on: [7.1, 7.2], touches: [] -->
