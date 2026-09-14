# Tasks — native-rendering wiring

## 1. Wire surfaces into EguiSurface::paint
- [ ] 1.1 Remove #[cfg(test)] gate on `mod physical/panels/viewer/picker/overlays` + re-exported Frame types in `src/gui/mod.rs`
- [ ] 1.2 Wire each draw routine into `EguiSurface::paint` dispatch (remove placeholder comment, delegate to `panels::paint_panels`, `physical::paint_physical`, `viewer::paint_viewer`, `picker::paint_picker`, `overlays::paint_*` with correct pane rects from App)
- [ ] 1.3 Keep headless egui tests green and add runtime smoke (one per surface asserting non-empty shapes)
- [ ] 1.4 Verify `openspec validate --specs` and `cargo test` remain green

## 2. Close duplicate bead on wiring
- [ ] 2.1 After 1.x lands, close `droid_tui-xqw` as duplicate of this change (or verify fix) and keep `droid_tui-hud`/`droid_tui-uin` lifecycle consistent
