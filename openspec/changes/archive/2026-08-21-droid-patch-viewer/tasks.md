# OpenSpec Tasks: droid-patch-viewer

Wave plan (file-disjointness caps concurrency at 2, not the configured 3):
`[1]` -> `[2]` -> `[3,4]` -> `[5]` -> `[6]` -> `[7,8]`

Note: task 4 has a hard API dependency on task 2's viewer state fields
(viewer_patch/viewer_selected_circuit/viewer_scroll/load_patch), so [2,4]
cannot run in parallel without a compile race; task 2 runs alone first.

Known gap (deferred, not blocking): tasks 5/6 spawn `droid_tui --view-source`,
but no task adds CLI arg parsing for that flag (see herdr-integration.md).
The binary will ignore the flag until a follow-up task adds it.

- [x] 1 Add `g` prefix key handling in `handler.rs` (detect `g`, start prefix mode, wait for next key) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [x] 2 Add viewer state to `app.rs` (`showing_viewer`, `viewer_patch`, `viewer_selected_circuit`) <!-- agent: rusty-engineer.build, depends_on: [1], touches: [src/app.rs] -->
- [x] 3 Add viewer render function in `ui.rs` (two-pane diff-viewer layout, sidebar with circuit jump points, chat-bubble circuit blocks) <!-- agent: layout-designer-engineer.build, depends_on: [2], touches: [src/ui.rs] -->
- [x] 4 Add key dispatch in `main.rs` for `g` prefix routing to viewer handler <!-- agent: rusty-engineer.build, depends_on: [1], touches: [src/main.rs] -->
- [x] 5 Add herdr integration logic (`herdr pane split` when `HERDR_ENV=1`) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1], touches: [src/handler.rs, src/app.rs] -->
- [x] 6 Add fallback second window logic (`kitty @ new-window` when not in herdr) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [5], touches: [src/handler.rs, src/app.rs] -->
- [x] 7 Create viewer spec files (`specs/viewer-layout.md`, `specs/keybinding.md`, `specs/herdr-integration.md`) <!-- agent: layout-designer-engineer.fast, depends_on: [3], touches: [openspec/specs/] -->
- [x] 8 Add viewer tests (ESC closes, sidebar navigation, circuit jump, readonly behavior) <!-- agent: horst-engineer.build, depends_on: [1,2,3,4], touches: [src/handler.rs, src/app.rs, src/ui.rs, tests/] -->

**Total: 8 tasks**
**Agent assignments**: rusty-engineer.build (1,2,4), layout-designer-engineer.build (3), layout-designer-engineer.fast (7), dermannmitdermachine-engineer.build (5,6), horst-engineer.build (8)
