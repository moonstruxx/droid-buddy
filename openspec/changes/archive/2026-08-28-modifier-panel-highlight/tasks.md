## 1. Influence cache & hue — domain foundation

- [x] 1.1 Build per-token structural `Influence` (hw_tokens/cables/circuits BFS) at `Patch::from_ini_str` and verify `B1.1`→`_TRIG`→`arpeggio` and multi-hop via switch are in the set, cycles terminate, and empty-token has no influence <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/patch.rs] -->
- [x] 1.2 Add pure `modifier_hue(token)` (`hash % 16`) helper and verify determinism, cycling at 17 tokens, and no new `Theme` fields/config keys are introduced <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/theme.rs, src/patch.rs] -->
- [x] 1.3 Cache In-module unit tests for BFS determinism (sorted iteration), cross-fixture (`arpeggio1.ini`, `source_navigation.ini`, `cable_banner_combos`) and verify `cargo test` passes <!-- agent: horst-engineer.build, depends_on: [1.1,1.2], touches: [src/patch.rs, src/theme.rs] -->

## 2. App state — held vs latched

- [x] 2.1 Add `App` state `influence_cache`, `pressed: Option<String>`, `latched: BTreeSet<String>` + insertion order, rebuild on `load_patch`, `Esc` clears both + `active_shift`, and verify via `App` unit tests <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->
- [x] 2.2 Add union helpers (`active_modifiers()`, `hue_for_cell(edge/span)`, status hint `MOD B1.1+B1.2 → N cells / M cables` with most-recent wins on overlap) and verify via `app.rs` tests <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/app.rs] -->

## 3. Interaction — mouse chords in handler

- [x] 3.1 Implement `Down` without mods = momentary (hit-test via `component_rects`, only modifier-eligible tokens), `Up`/`Leave` clears, and verify via `handler::handle_mouse_event` tests with synthetic `MouseEvent` <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/handler.rs] -->
- [x] 3.2 Implement `Ctrl+Shift+Click` toggle latch (also accept `Ctrl+Click` alias), additive union, `Esc` clears all, and verify via handler tests including `KeyModifiers` chord detection <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs] -->
- [x] 3.3 Add keyboard alias `m` for hovered component in main view to toggle latch (accessibility) and verify via `handle_event` test <!-- agent: rusty-engineer.build, depends_on: [3.2], touches: [src/handler.rs] -->

## 4. Rendering — main panels, source, graph, status

- [x] 4.1 Render influenced cells with background wash in modifier hue (boxed + text cells, unaffected dimmed), orthogonal to shift panel border, and verify via `ui.rs` `TestBackend` unit tests per theme (classic/mono/terminal) <!-- agent: layout-designer-engineer.build, depends_on: [2.2], touches: [src/ui.rs] -->
- [x] 4.2 Recolor source `ModifierAffect` spans and graph edges/nodes with same hue (error red > modifier hue > CableKind), additive union most-recent wins, and verify via `ui.rs` buffer tests and graph snapshot deltas <!-- agent: layout-designer-engineer.build, depends_on: [4.1], touches: [src/ui.rs, src/theme.rs] -->
- [x] 4.3 Render status hint `MOD <tokens> → N cells / M cables` in modifier hue and coexistence case shift border + modifier bg, and verify via `rendered_text` assertions <!-- agent: layout-designer-engineer.build, depends_on: [4.2], touches: [src/ui.rs] -->

## 5. Tests, snapshots & gallery

- [x] 5.1 Add regression harness for modifier highlight: fixtures `arpeggio1`, `source_navigation`, `cable_banner_combos` × themes × widths 80/120, latched vs held, additive, shift+modifier, and verify `cargo insta test --check` passes <!-- agent: horst-engineer.build, depends_on: [4.3], touches: [src/regression.rs, src/snapshots/**] -->
- [x] 5.2 Regenerate ephemeral gallery (`cargo run --bin snapshot-gallery` or `cargo test -- --generate-gallery`) and verify `evidence/gallery/index.html` shows per-modifier washes <!-- agent: horst-engineer.build, depends_on: [5.1], touches: [evidence/gallery/**, src/gallery.rs] -->
- [x] 5.3 Full verification: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo insta test --check`, `cargo build --release --locked` and verify zero warnings <!-- agent: horst-engineer.fast, depends_on: [5.2], touches: [] -->

## 6. Docs

- [x] 6.1 Regenerate `ARCHITECTURE.md`/`DESIGN.md` and guardrails via `/make-architecture`/`/make-design`/`/make-guardrails` and verify docs mention modifier-panel-highlight chord and rendering priority <!-- agent: rusty-engineer.fast, depends_on: [5.3], touches: [ARCHITECTURE.md, DESIGN.md, .agents/skills/ob-guardrails-project/SKILL.md] -->
