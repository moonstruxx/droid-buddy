# Tasks

## 1. Model

- [ ] 1.1 Verify per-device LED wiring against the DROID manual and the RAGFlow droid knowledge base (B32, P2B8, P4B2, M4, E4, P8S8, DB8E, master, master18, G8, X7): which LED belongs to which element, RGB vs white-only, and which colour registers set each LED's colour. Record the findings in design.md under D2/D3 <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [openspec/changes/module-ui-physical/design.md] -->
- [ ] 1.2 Add optional RGB colour to the LED state, set from the patch's colour registers, plus a per-controller rgb/white capability <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs] -->
- [ ] 1.3 Add a pure LED→element resolver: explicit `led`/`ledN` pairing first, then the positional default per controller (E4/DB8E rings of 32, M4 touch plates, jack LEDs for master/G8/X7) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs, src/physical.rs, controller_geometry.json] -->
- [ ] 1.4 Enumerate rack elements from `controller_geometry.json` element cells (so unused elements exist) and add the master/master18 as rack modules at chain position 0, replacing the CV I/O group <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.3], touches: [src/physical.rs] -->

## 2. Physical view becomes the module UI

- [ ] 2.1 Render LEDs inside their element (button face, M4 touch plate, E4 segmented ring, P8S8 slider track, jack-side LED) and remove standalone/co-located LED cells <!-- agent: layout-designer-engineer.build, depends_on: [1.2, 1.3], touches: [src/gui/physical.rs] -->
- [ ] 2.2 Render every hardware element including unused ones (dimmed, unlabelled) and the master faceplate <!-- agent: layout-designer-engineer.build, depends_on: [1.4, 2.1], touches: [src/gui/physical.rs] -->
- [ ] 2.3 Show shift-aware labels on every used element via `display_label`; ellipsize long labels and show the full label on hover and in the status bar <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/physical.rs, src/app.rs] -->
- [ ] 2.4 Open the view fitted to pane width (on open and patch load) using the published pane size; keep zoom presets and pan <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/gui/physical.rs, src/app.rs] -->

## 3. Retire Panels

- [ ] 3.1 Remove `ViewType::Panels`: `BIG_CAROUSEL` = [Graph, Physical], startup puts Physical in BigLeft, delete `src/gui/panels.rs` and its dispatch <!-- agent: rusty-engineer.build, depends_on: [2.2], touches: [src/panes.rs, src/app.rs, src/gui/mod.rs, src/gui/panels.rs, src/main.rs] -->
- [ ] 3.2 Move every Panels interaction onto the physical view: element navigation (`j`/`k`/arrows when the rack fits), Enter/Space toggle, wheel on knobs/faders, `m` latch, `e` label editor, selected-circuit highlight, modifier wash, shift `1`–`4` <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs, src/app.rs] -->
- [ ] 3.3 Merge the Panels help table into the module UI (Physical) help view <!-- agent: rusty-engineer.fast, depends_on: [3.2], touches: [src/help.rs] -->

## 4. Verification

- [ ] 4.1 Regression tests: no standalone LED cells per controller, E4 ring from L registers, RGB vs white-only, unused elements drawn, labels follow shift, carousel without Panels, every 3.2 interaction on the physical view; port the Panels tests <!-- agent: horst-engineer.build, depends_on: [2.3, 2.4, 3.2], touches: [src/regression.rs, src/gui/physical.rs, src/handler.rs, src/app.rs] -->
- [ ] 4.2 Live before/after capture of the module UI (xdotool windowfocus + import) with a per-module cell count (e.g. B32 = 32, zero LED cells) <!-- agent: horst-engineer.fast, depends_on: [4.1], touches: [] -->
- [ ] 4.3 Run the four gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` <!-- agent: horst-engineer.fast, depends_on: [4.1], touches: [] -->
