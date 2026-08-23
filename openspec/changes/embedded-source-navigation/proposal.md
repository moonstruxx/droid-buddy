## Why

The source viewer opened via `g v` runs as a second process instance (herdr pane or terminal-emulator window), so it cannot react to what the user does in the main TUI: selecting a component cannot jump to its source, and modifier relationships between components are invisible. The viewer also renders reconstructed circuit boxes rather than line-accurate patch source and keeps no occurrence index, so there is no way to navigate from a hardware component to every place it appears in the `.ini`.

Embedding the source pane in the main process closes that gap: panel state and source state live in one `App`, jumps are instant, and a small index enables occurrence navigation and modifier highlighting without IPC or background services.

## What Changes

- **BREAKING**: `g v` opens an embedded source pane inside the main TUI instead of spawning a herdr pane or fallback terminal window. Herdr integration, fallback window spawning, `ViewerMode`, and the `serde_json` dependency are removed.
- Source pane shows line-accurate raw `.ini` text; `t` toggles between raw view and the existing prettified circuit blocks.
- Components gain explicit selection (distinct from hover): Enter/Space/click toggles and selects; clicking empty panel space clears selection.
- Selecting a component jumps the source pane to its first occurrence; selecting another component jumps to its first occurrence; clearing selection preserves the current position. Initial viewer position is beginning of file when nothing is selected.
- Occurrence navigation in the source pane: `j`/`k` scroll lines, Up/Down go to previous/next occurrence of the selected component, Home/End go to first/last occurrence.
- Parser retains raw lines and per-token line ranges; minimal cached indexes cover sections, hardware-token occurrences, and modifier relationships.
- Modifier analysis resolves which source fragments react to a selected component:
  - `select = X` as boolean activation from positive `X`
  - `select = X` with `selectat = N` as exact-value activation when `X == N`
  - direct hardware sources such as `select = B1.2`
  - internal cable producers and transitive chains
  - cycle-safe traversal
- Selecting a component highlights every affected source fragment; a full-file minimap with viewport indicator supports click-to-scroll.

## Capabilities

### New Capabilities

- `source-navigation`: Line-accurate source model (raw lines, section/token spans, occurrence and modifier indexes), selection-driven jumps, occurrence navigation, deselection stability, and modifier-graph highlighting.

### Modified Capabilities

- `viewer-layout`: Viewer becomes an embedded split pane of the main TUI with focus styling, raw/prettified toggle, highlight rendering, and a full-file minimap; herdr/fallback window layouts are removed.
- `keybinding`: `g v` opens the embedded pane; `t` toggles view mode; Tab switches focus between panels and source pane; Up/Down/Home/End gain occurrence-navigation meaning while the pane is focused; readonly-until-Esc semantics carry over to the embedded pane.
- `patch-parsing`: Parser additionally preserves raw source lines and records line ranges for sections and hardware tokens, and exposes the minimal occurrence/modifier indexes.
- `mouse-interaction`: Component clicks select in addition to toggling; clicking empty panel space clears selection; clicking the minimap scrolls the source pane.
- `herdr-integration`: Capability removed — no requirement of it remains after this change.

## Impact

- `src/patch.rs`: retain raw lines, add span/index types (`SourceMap`, occurrence index, modifier graph) consumed by app/handler/ui; parser tests extended.
- `src/app.rs`: explicit selection field, pane focus, raw/prettified mode, occurrence cursor, source scroll, minimap geometry; removal of duplicated viewer projection state and `ViewerMode`.
- `src/handler.rs`: embedded viewer keys, selection/deselection wiring, minimap hit-testing; deletion of launcher helpers (`open_viewer_window`, `parse_herdr_pane_id`, `determine_fallback_terminal_cmd`) and their tests.
- `src/ui.rs`: embedded split layout, both view modes, highlight + minimap rendering; publishes minimap geometry alongside `component_rects`.
- `Cargo.toml` / `Cargo.lock`: drop `serde_json` once its last consumer is gone.
- Docs (derived): regenerate `ARCHITECTURE.md` / `DESIGN.md` / project guardrails after implementation.
- Specs: new `source-navigation`; deltas for `viewer-layout`, `keybinding`, `patch-parsing`, `mouse-interaction`; removal delta for `herdr-integration`.

## Non-goals

- No general text search, source editing, or saving from the source pane.
- No arbitrary DROID expression evaluation beyond the `select`/`selectat` modifier forms listed above.
- No hardware/MIDI state reflection; the app stays a local simulator.
- No cross-process synchronization — there is no second process anymore.
- No persistence of selection, scroll, or view mode across sessions (YAGNI).
- No schema validation against `circuits.json` in this change.
