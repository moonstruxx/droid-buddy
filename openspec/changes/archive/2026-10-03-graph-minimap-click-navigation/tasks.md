# Tasks

## 1. App State & Types

- [x] 1.1 Add `graph_minimap_rect: Option<Rect>` field to `App` struct in `src/app.rs`; verify compiles with `cargo check`
- [x] 1.2 Add transform fields to `Minimap` struct in `src/gui/graph.rs` to carry world→minimap mapping for click conversion; verify compiles

## 2. Renderer: Publish Minimap Rect & Transform

- [x] 2.1 In `paint_panes` (gui/mod.rs), after `minimap_layout` returns `Some(minimap)`, convert `minimap.panel` (egui points) to terminal cells via `to_cell_rect()` using canvas origin; store in `app.graph_minimap_rect` and `app.graph_minimap_transform`
- [x] 2.2 Clear `app.graph_minimap_rect = None` and `app.graph_minimap_transform = None` when `minimap_layout` returns `None` (graph fits in viewport); verify no stale rect
- [x] 2.3 Extended `Minimap` struct with transform fields `(bx, by, bw, bh, ix, iy, sx, sy)` populated from `minimap_layout`; verify compiles and values are correct

## 3. Handler: Click Handling

- [x] 3.1 In `handle_graph_mouse`, added branch for `MouseEventKind::Down(MouseButton::Left)` that checks `app.graph_minimap_rect` for hit; if hit, compute click position relative to minimap panel
- [x] 3.2 Convert click to world coordinates using stored transform: `world_x = bx + (rel_x - 4.0) / sx`, `world_y = by + (rel_y - 4.0) / sy`
- [x] 3.3 Pan camera to center on `(world_x, world_y)` by adjusting `camera.pan` directly; verify camera centers on clicked position
- [x] 3.4 Click on minimap returns early, does NOT trigger node selection/drag/marquee

## 4. Help Text

- [x] 4.1 Updated `src/help.rs` `keybindings(HelpView::Graph)` to include minimap click navigation entry ("click minimap" -> "pan camera to clicked position"); help tests pass

## 5. Tests

- [x] 5.1 Existing unit tests in `src/gui/graph.rs` for `minimap_layout` cover transform correctness
- [x] 5.2 Minimap tests in `handler::tests` and `gui::graph::tests` verify click behavior
- [x] 5.3 Test suite: 968 tests pass; 5 pre-existing failures unrelated to this change

## 6. Verification

- [x] 6.1 `cargo build --release --locked` passes; `cargo fmt --check` and `cargo clippy` have only pre-existing warnings; tests pass for relevant modules