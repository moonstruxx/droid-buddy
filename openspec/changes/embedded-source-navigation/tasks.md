# OpenSpec Tasks: embedded-source-navigation

Wave plan (file-disjointness caps concurrency at 2 within the configured maxConcurrent: 3):
`[1.1]` -> `[1.2]` -> `[2.1]` -> `[3.1]` -> `[3.2, 4.1]` -> `[4.2]` -> `[3.3]` -> `[5.1]` -> `[6.1, 6.2]`

Notes:
- Tasks 1.1/1.2 share `src/patch.rs` and the fixture — strictly sequential.
- Task 3.1 deletes the launcher helpers and drops `serde_json`; verify no remaining consumer
  (`grep serde_json src/ Cargo.toml`) before removing from `Cargo.toml`, then `cargo build --locked`
  refreshes `Cargo.lock`.
- Renderer-published minimap geometry extends the `component_rects` handoff contract (design D6);
  task 3.3 consumes what task 4.2 publishes.

## 1. Parser: line-accurate source model

- [x] 1.1 Retain raw `.ini` lines verbatim and record spans (line + column range) for section headers and boundary-aware hardware-token hits during the parse pass; create `fixtures/source_navigation.ini` covering repeated sections, comments, internal variables, `select`/`selectat` forms. Verify: parser tests pass for raw-line round-trip, span positions, `_ENV…` false-positive guard, and unchanged `arpeggio1.ini` results <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs, fixtures/source_navigation.ini] -->
- [x] 1.2 Build occurrence index (token -> ordered occurrences) and cycle-safe select/selectat modifier graph (boolean, exact-value, direct hardware source, transitive internal producers) as `Patch` fields with named consumers only. Verify: tests for reading-order occurrences, each resolution form, cyclic termination, unknown-token empty result <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs, fixtures/source_navigation.ini] -->

## 2. Application state

- [x] 2.1 Rework viewer state: explicit `selected_component`, pane focus flag/enum, raw/prettified mode, occurrence cursor, source scroll, minimap geometry slot; initialize from indexes at `load_patch`; delete `ViewerMode` and duplicated viewer projection state. Verify: unit tests for defaults, load reset behavior, and that no removed fields are referenced (`cargo build --all-targets --locked`) <!-- agent: rusty-engineer.build, depends_on: [1.1, 1.2], touches: [src/app.rs] -->

## 3. Input handling

- [x] 3.1 Route `g v` to the embedded pane (focus source, apply initial-position rule), `t` toggles modes, `Tab` switches focus, `Esc` closes keeping selection; enforce viewer-focus input isolation (panel toggles/shift/scale/orientation inert until Tab/Esc); delete `open_viewer_window`, `parse_herdr_pane_id`, `determine_fallback_terminal_cmd`, their tests, and remove `serde_json` if unconsumed. Verify: handler tests for open/close/toggle/focus/isolation; `grep -r "serde_json\|open_viewer_window\|ViewerMode" src/` returns nothing; `cargo build --locked` passes <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/handler.rs, Cargo.toml, Cargo.lock] -->
- [x] 3.2 Wire selection into commit interactions: Enter/Space/click toggles AND selects; replacement selection re-jumps; empty-panel-space click clears selection without moving source scroll; Up/Down/Home/End occurrence navigation saturating at bounds. Verify: handler tests through `handle_event` for each interaction incl. deselection stability and no-selection no-op <!-- agent: rusty-engineer.build, depends_on: [3.1], touches: [src/handler.rs] -->
- [x] 3.3 Implement minimap click-to-scroll using renderer-published minimap geometry (shared proportional mapping with the indicator). Verify: handler test clicks the published rect at fractions of height and asserts resulting scroll lines <!-- agent: rusty-engineer.build, depends_on: [3.2, 4.2], touches: [src/handler.rs] -->

## 4. Rendering

- [x] 4.1 Render embedded split (panels | sidebar | source) inside the main skeleton with focus border emphasis, picker precedence, raw default + prettified blocks, sidebar disambiguation, viewer status hints (ESC/j-k/Up-Down/t/Tab), empty-patch message, narrow-terminal minima. Verify: `render` frame tests at wide/narrow sizes for both modes and focus states <!-- agent: layout-designer-engineer.build, depends_on: [2.1], touches: [src/ui.rs] -->
- [x] 4.2 Render selection/occurrence/modifier highlights in the source area and the full-file minimap with viewport indicator; publish minimap geometry alongside `component_rects`; hide minimap below width threshold. Verify: frame tests assert highlight spans for direct/transitive/exact-value cases, indicator position tracks scroll, geometry published, hidden-on-narrow <!-- agent: layout-designer-engineer.build, depends_on: [3.2, 4.1], touches: [src/ui.rs] -->

## 5. Cross-layer regression

- [x] 5.1 Add regression suite driving real flows end-to-end through `handle_event` + `render` with the fixture: initial BOF vs selected-open, first/replacement jumps, occurrence bounds, deselect keeps position, modifier highlights appear/clear, `t` preserves usable content, minimap click maps correctly, Tab focus round-trip, picker precedence, viewer isolation. Verify: full `cargo test` green including new suite <!-- agent: horst-engineer.build, depends_on: [3.3, 4.2], touches: [src/app.rs, src/handler.rs, src/patch.rs, src/ui.rs, fixtures/source_navigation.ini] -->

## 6. Derived docs & quality gates

- [x] 6.1 Regenerate ARCHITECTURE.md (`/make-architecture`) and ob-guardrails-project (`/make-guardrails`) to describe the embedded pane, span/index model, and removals (herdr/fallback/serde_json/`--view-source`). Verify: no stale second-process/herdr statements remain; rule counts reported by the skills reflect the new architecture <!-- agent: rusty-engineer.fast, depends_on: [5.1], touches: [ARCHITECTURE.md, .agents/skills/ob-guardrails-project/SKILL.md] -->
- [x] 6.2 Regenerate DESIGN.md (`/make-design`) for new tokens/layout (focus emphasis, minimap, highlights), then run full gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`. Verify: all four commands exit 0 <!-- agent: layout-designer-engineer.fast, depends_on: [5.1, 6.1], touches: [DESIGN.md] -->
