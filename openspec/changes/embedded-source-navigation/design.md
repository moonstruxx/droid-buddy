# Design: embedded-source-navigation

## Context

The source viewer currently runs as a second `droid_tui --view-source` process in a herdr pane or terminal-emulator window (`handler.rs`: `open_viewer_window`, `parse_herdr_pane_id`, `determine_fallback_terminal_cmd`; app state: `ViewerMode`; dep: `serde_json` for herdr JSON). A separate process cannot share selection or highlight state, which is the core of this change (see proposal.md — Why).

Current shape relevant to the design:

- Parser (`patch.rs`) is a pure function over strings producing `Patch` (components, sections, shift groups) plus `viewer_circuits()` projection; it discards line positions.
- Renderer owns layout and publishes geometry back to state via `app.component_rects` — the one renderer→handler collaboration contract.
- Handler priority order: picker → armed prefix → viewer (readonly) → normal keys.
- Semantic anchors: YAGNI, DRY, CodeAsDoc, TDD London through public entry points (`handle_event`, `render`) with real fixtures.

## Goals / Non-Goals

**Goals:**

- One process, one `App`: panels and source pane share state so selection instantly drives source position and highlights.
- Line-accurate raw view backed by spans recorded during parsing (single pass).
- Minimal indexes with current consumers only: token occurrences, section starts, select/selectat modifier graph.
- Preserve existing interaction contracts: toggle-on-click/Enter stays; picker precedence; fresh-layout-per-frame.

**Non-Goals:**

- General text search/editing, full expression evaluation, hardware/MIDI reflection, persistence across sessions (see proposal Non-goals).
- No new dependencies; one is removed (`serde_json`).

## Decisions

### D1: Embed the pane; delete the second-process path entirely

Replace `open_viewer_window` + herdr/fallback helpers and `ViewerMode` with an embedded split pane. Remove `serde_json` when its last consumer is gone.

- *Why*: every feature in this change (selection jump, modifier highlights, minimap) needs shared state; IPC/file-watching between two processes would add a protocol, race conditions, and a dependency — all rejected by YAGNI against a one-crate monolith that already shares `App`.
- *Alternative considered*: keep herdr pane and sync state via temp files/watchers — rejected: fragile, latency between toggle and highlight, and the `--view-source` flag is still unconsumed debt today.

### D2: Record spans during the parse pass; store them on `Patch`

Extend the hand-rolled scanner to capture `(line, col_range)` per section header and per boundary-aware hardware-token hit, retaining raw lines verbatim. Indexes (token → ordered occurrences; section → start span; modifier graph) are plain fields built once at load time on `Patch`.

- *Why*: the scanner already walks values token-by-token; capturing positions there is a single source of truth (DRY) and keeps the parser pure. Lazy re-tokenization in ui/handler would duplicate the token grammar.
- *Alternative considered*: post-hoc scan of retained lines — rejected: second implementation of the same grammar risks divergence with the component model.
- *Consumer note*: no speculative index types — each index names its consumer (occurrences → occurrence navigation & jumps; sections → sidebar Enter jump; modifiers → highlight rendering).

### D3: Modifier resolution limited to `select`/`selectat`, transitively through internal producers

Resolve assignments of the form `select = X [`selectat = N`]` where `X` is a direct hardware token or resolves through internal cable/variable definitions to one. Worklist traversal with a visited set (cycle-safe). Resolution is advisory: it drives highlights only, never state.

- *Why*: covers the approved behavior exactly; anything broader (arithmetic expressions, conditionals) is a full patch interpreter — explicitly out of scope.
- *Alternative considered*: evaluate against live component values at render time — partially kept: `selectat` equality needs the selected knob's current value at query time, but graph construction stays load-time.

### D4: Explicit selection as a new `App` field, set on commit-style interactions

`selected_component: Option<usize>` distinct from `hovered_component`. Enter/Space/click on a component toggles (unchanged) **and** selects; clicking empty panel space clears selection without touching source scroll; viewer `Esc` closes the pane but keeps the selection.

- *Why*: hover already exists and follows the mouse; conflating them would make highlights flicker under cursor movement and break keyboard-only workflows.
- *Alternative considered*: reuse hover as selection — rejected above; separate "selection mode" key first — rejected: adds a step before every jump.

### D5: Raw view default, prettified behind `t`

Raw verbatim text is the default because line-accurate navigation is the point; `t` flips to the existing chat-bubble blocks. Sidebar remains meaningful in both modes (circuits map to section start lines); `Enter` jumps to the selected entry's start in either mode.

- *Alternative considered*: prettified default to preserve today's look — rejected: today's look lives in a dead-end external window; users opening the viewer now expect source.

### D6: Layout = right split + focus bit; minimap degrades gracefully

Embedded layout reuses the header/main/status skeleton: main splits into panels | sidebar | source (+ minimap column). A `focus` enum picks which side receives keys; borders signal focus. Minimap hidden below a width threshold instead of squeezing source. Renderer publishes minimap rect(s) alongside `component_rects` — extending the established geometry-handoff contract rather than inventing a channel.

- *Alternative considered*: overlay/floating source window — rejected: obscures panels, complicates hit-testing.

### D7: Key routing keeps the existing priority chain

Priority becomes: picker → armed prefix → viewer-focused keys (j/k scroll, Up/Down/Home/End occurrences, t, Tab, Esc) → normal keys. While viewer is focused, panel keys are inert except Tab/Esc; `1–4`, `+/-`, `o`, Enter/Space/click require panel focus.

- *Why*: single dispatch site, matches current structure; readonly isolation is then a property of focus, not scattered guards.

## Risks / Trade-offs

- [Parser regression from span tracking] → spans are additive outputs; fixture-driven tests assert identical components/sections plus recorded spans (`fixtures/arpeggio1.ini` + new `fixtures/source_navigation.ini`).
- [Modifier chains mis-resolve exotic patches] → resolution is highlight-only advisory output; cycle-safe visited-set guarantees termination; unknown forms resolve to "no highlight" rather than wrong highlight.
- [Users relied on herdr/external-window viewing] → removal is BREAKING by proposal; embedded pane supersedes the use case; migration documented in the herdr delta.
- [Minimap click mapping drift vs viewport indicator] → one mapping function shared by click handling and indicator drawing, tested through `render` + mouse events.
- [Larger `Patch` (raw text duplicated)] → patches are kilobytes; acceptable; no caching layers introduced.

## Migration Plan

1. Implement parser/app/handler/ui changes on a branch; launcher deletion and dependency removal land with the handler task so the tree always builds.
2. Update tests: remove herdr/fallback tests, add navigation/highlight/minimap tests (TDD London through `handle_event`/`render`).
3. Archive: sync deltas (viewer-layout/keybinding/patch-parsing/mouse-interaction modified; herdr-integration requirements removed and spec directory retired), regenerate ARCHITECTURE.md / DESIGN.md / guardrails via `/make-*` (derived artifacts, never hand-edited).
4. Rollback: revert the branch; no data or format migration exists (nothing persisted).

## Open Questions

None blocking. `selectat` float-equality uses exact comparison against parsed literal values (no epsilon) unless fixture testing shows DROID emits imprecise literals; revisit only with evidence.
