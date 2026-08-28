## 1. Config & domain foundation

- [x] 1.1 Add `[labels]` config (`layers_enabled=true`, `max_shift_layer=4` clamped 1..8) to `config.rs` with XDG load/save, warn-once and defaults <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/config.rs] -->
- [x] 1.2 Add pure `Patch::display_label` + `circuit_label` with fallback chain `store[layer]→store[1]→preamble[1]→derived` and `max_shift_layer`/`layers_enabled` coercion in `patch.rs` <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs] -->
- [x] 1.3 Add `LabelStore` model (XDG `labels.toml`, per-patch buckets `hw` + `circuits`, atomic tmp→rename, path-key canonicalization, warn-once) + `I4:` empty-slot coverage <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2], touches: [src/app.rs] -->

## 2. App state & persistence

- [x] 2.1 Wire `LabelStore` into `App` (`label_store`, `editing: Option<EditState>`, `load_patch` bucket load, `recompute_influence` for overlay status) <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/app.rs] -->
- [ ] 2.2 Add overlay draft lifecycle (`Enter` save + atomic rewrite, `Esc` cancel, `1..N` layer cycle preserving per-layer drafts) with unit tests <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/app.rs] -->

## 3. Interaction — overlay as event eater

- [ ] 3.1 Implement overlay-eating priority in `handler.rs` (overlay > picker > prefix > graph > source > panels) and `e` entry for focused panel token / source header instance / hovered graph node <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/handler.rs] -->
- [ ] 3.2 Implement in-overlay keys (char append/delete, `1..N` layer switch, `Enter`/`Esc`, arrows) and save path <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs] -->
- [ ] 3.3 Surface status `B3.17 / Group2 → N ckts / M cables` with structural hue and clamp handling <!-- agent: layout-designer-engineer.build, depends_on: [2.2], touches: [src/handler.rs, src/app.rs] -->

## 4. Rendering — overrides + overlay

- [ ] 4.1 Override HW panel cell labels via `display_label` (boxed + TextCell, `layers_enabled`/`max` aware) preserving geometry/`component_rects` <!-- agent: layout-designer-engineer.build, depends_on: [1.2, 2.1], touches: [src/ui.rs] -->
- [ ] 4.2 Override source header and graph node titles via circuit label (FULL+FILTERED) <!-- agent: layout-designer-engineer.build, depends_on: [4.1], touches: [src/ui.rs] -->
- [ ] 4.3 Render centered single-field overlay z-layer (1-line input + hint in modifier hue, `graph_edge_error` red precedence kept) responsive per width <!-- agent: layout-designer-engineer.build, depends_on: [3.2], touches: [src/ui.rs] -->

## 5. Tests, snapshots & gallery

- [ ] 5.1 Unit tests for `display_label` fallback/clamp/disabled coercion, store round-trip, circuit override, and `I4:` empty fixture <!-- agent: horst-engineer.build, depends_on: [1.2, 1.3], touches: [src/patch.rs, src/config.rs, src/app.rs] -->
- [ ] 5.2 Regression `TestBackend` snapshots for overlay + label overrides per theme/width (panels/source/graph, Group2, disabled) <!-- agent: horst-engineer.build, depends_on: [4.3], touches: [src/regression.rs, src/snapshots/**] -->
- [ ] 5.3 Regenerate `evidence/gallery` and verify `cargo fmt --check` + `cargo clippy` + `cargo test --locked` + `cargo insta test --check` + `cargo build --release --locked` zero warnings <!-- agent: horst-engineer.fast, depends_on: [5.2], touches: [] -->

## 6. Docs

- [ ] 6.1 Regenerate `ARCHITECTURE.md`/`DESIGN.md` and guardrails for labels overlay and `[labels]` config <!-- agent: rusty-engineer.fast, depends_on: [5.3], touches: [ARCHITECTURE.md, DESIGN.md, .agents/skills/ob-guardrails-project/SKILL.md] -->
