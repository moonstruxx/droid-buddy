## 1. Paint Function Restructure

- [x] 1.1 Change `paint_scene` signature from `painter: &Painter` to `ui: &mut Ui` in src/gui/graph.rs, update all call sites in src/gui/mod.rs, and verify existing graph tests pass
- [x] 1.2 Change `paint_panels` signature from `painter: &Painter` to `ui: &mut Ui` in src/gui/panels.rs, update all call sites in src/gui/mod.rs, and verify existing panel tests pass
- [x] 1.3 Change `paint_physical` signature from `painter: &Painter` to `ui: &mut Ui` in src/gui/physical.rs, update all call sites in src/gui/mod.rs, and verify existing physical tests pass

## 2. AccessKit Annotations

- [x] 2.1 Add `ui.allocate_exact_size()` + `response.widget_info(WidgetInfo::labeled(...))` for each graph node in paint_scene, and verify annotation appears in AccessKit tree via a kittest test
- [x] 2.2 Add `ui.allocate_exact_size()` + `response.widget_info(WidgetInfo::labeled(...))` for each panel component cell in paint_panels, and verify annotation appears in AccessKit tree via a kittest test
- [x] 2.3 Add `ui.allocate_exact_size()` + `response.widget_info(WidgetInfo::labeled(...))` for each physical element cell in paint_physical, and verify annotation appears in AccessKit tree via a kittest test

## 3. kittest Test Expansion

- [x] 3.1 Add end-to-end journey test: load patch → open graph → verify nodes → close graph → reopen, and verify all tests pass
- [x] 3.2 Add end-to-end journey test: load patch → open source viewer → navigate occurrences → close viewer, and verify all tests pass
- [x] 3.3 Add end-to-end journey test: load patch → open physical view → toggle skeleton → adjust zoom, and verify all tests pass
- [x] 3.4 Add end-to-end journey test: load patch → open optimizer → navigate candidates → preview/restore, and verify all tests pass

## 4. Verification

- [x] 4.1 Run full verification gate: cargo fmt --check, cargo clippy --all-targets --all-features --locked -- -D warnings, cargo test --locked, cargo build --release --locked
