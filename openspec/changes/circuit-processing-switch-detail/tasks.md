## 1. Switch token & value rendering — foundation

- [x] 1.1 Add `switch` semantic token to `Theme` + palettes (`classic`=white byte-identical, `terminal`=Reset, `mono`=dark-gray distinct from button gray) and verify per-palette resolution via `theme.rs` tests <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/theme.rs] -->
- [x] 1.2 Render Switch cells with the `switch` token and `◉ {:.0}%` for `ComponentState::Value`, keeping `▣ ON`/`□ OFF` for On/Off, and verify per-theme `TestBackend` buffer tests <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/ui.rs] -->
- [x] 1.3 Add/extend regression snapshots for switch value rendering across fixtures × classic/mono and verify `cargo insta test --check` passes <!-- agent: horst-engineer.build, depends_on: [1.2], touches: [src/regression.rs, src/snapshots/**] -->

## 2. Global processing pause

- [x] 2.1 Add `App.processing_paused: bool` + toggle with status messages; block state-mutating handlers while paused (`p` key toggles; Enter/Space toggles, mouse toggles, knob/fader scroll blocked; selection/navigation/picker/prefix unaffected; influence cleared while paused) and verify via `app.rs`/`handler.rs` tests <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs, src/handler.rs] -->
- [x] 2.2 Render the panel main area dimmed while paused with status bar `PROCESSING PAUSED`, header/status bars normal, geometry unchanged, and verify via `ui.rs` buffer tests per theme <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs] -->

## 3. Per-circuit processing toggle

- [x] 3.1 Add `App.disabled_circuits: HashSet<(String, usize)>`; make the influence walk treat a disabled circuit as a dead end (its own cells stay influenced, outputs do not propagate); reset on `load_patch`; verify via `patch.rs`/`app.rs` unit tests <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/app.rs, src/patch.rs] -->
- [x] 3.2 Graph surface `x` key toggles processing for the hovered node's circuit (hit-test via `graph_node_rects`), rebuilds graph, recomputes influence, emits `GraphRebuilt`, shows status naming the circuit; no-hover is a silent no-op; verify via `handler.rs` tests <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs, src/app.rs] -->
- [x] 3.3 Render disabled graph nodes and incident edges dim (overriding influence highlight, hover styling kept) and verify via `ui.rs` buffer tests and graph snapshot deltas <!-- agent: layout-designer-engineer.build, depends_on: [3.2], touches: [src/ui.rs] -->

## 4. Tests, snapshots & gallery

- [x] 4.1 Add regression harness for paused-dim and disabled-circuit scenarios (pause toggle, disabled node/edge dim, influence cut) × themes × widths, verify `cargo insta test --check` <!-- agent: horst-engineer.build, depends_on: [2.2, 3.3], touches: [src/regression.rs, src/snapshots/**] -->
- [x] 4.2 Regenerate ephemeral gallery (`cargo run --bin snapshot-gallery` or `cargo test -- --generate-gallery`) and verify `evidence/gallery/index.html` shows paused-dim and switch-value rows <!-- agent: horst-engineer.build, depends_on: [4.1], touches: [evidence/gallery/**, src/gallery.rs] -->
- [ ] 4.3 Full verification: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo insta test --check`, `cargo build --release --locked` and verify zero warnings <!-- agent: horst-engineer.fast, depends_on: [4.2], touches: [] -->

## 5. Docs

- [ ] 5.1 Regenerate `ARCHITECTURE.md`/`DESIGN.md` and guardrails via `/make-architecture`/`/make-design`/`/make-guardrails` and verify docs mention processing pause (`p`), per-circuit toggle (`x`), and the switch token/value rendering <!-- agent: rusty-engineer.fast, depends_on: [4.3], touches: [ARCHITECTURE.md, DESIGN.md, .agents/skills/ob-guardrails-project/SKILL.md] -->