## Why

Today `droid_tui` shows one exclusive surface at a time in its main band — panels, source viewer, graph, physical layout, or validation modal. Debugging a patch means switching between views repeatedly: pick a modifier in panels, open the source viewer to see `select` sites, switch to the graph to find influenced circuits, then back again. Each view replacement erases the context the user just built.

The quad view (archive `modifier-flow-quad-view`) proved the value of co-visible signals but went too far in the other direction — it replaced the entire main band with a rigid 2×2 layout, locked to modifier tracing, and did not generalize to other multi-view workflows.

A tiling window manager generalizes the idea: any combination of views can share the screen, with a carousel to rotate through open views, focus routing to keep keyboard/mouse context-aware, and a narrow-terminal fallback. The optimizer graduates from a centered modal (which blocks the main view while you compare candidates) to a side pane. All zoom-like actions (`+`/`-` for panels, `[`/`]` for split ratio, graph camera zoom, cable tension) consolidate onto a single key family with modifiers so they work regardless of which pane has focus.

## What Changes

- **Tiling main band.** The main area (between header and status) keeps a fixed left pane that always shows the component panels. The right side becomes a vertical stack where open views (graph, source viewer, physical layout) are placed with horizontal cuts only. Only one view occupies a slot at a time; a carousel (`C-a` / `C-x` or `Shift+Tab` / `Tab`) rotates through open views in each slot.
- **Focus follows selection.** `Tab` cycles focus across visible panes. Keys route to the focused view (graph keys when graph has focus, viewer keys when viewer has focus, etc.). Mouse clicks set focus on the clicked pane.
- **Optimizer promoted.** The latency optimizer (`g o`) moves from a centered blocking modal to a right-side pane, letting the main panel view remain visible while you compare candidates and preview reorderings.
- **Quad view replaced.** The permanent quad view (`g q`) is removed. A vertical split inside the main left pane survives as an optional toggle (`\`) for side-by-side comparison, using the same carousel/slot mechanism.
- **Zoom family consolidated.** `+`/`-` scale the focused pane by default (panels scale or graph camera zoom depending on what's focused). `Shift++`/`Shift+-` scale the other (non-focused) pane. `[`/`]` adjust the split ratio between left/right. `Alt+[`/`Alt+]` adjust cable tension when graph has focus. The old `[`/`]` viewer-split key is removed (the new split ratio key replaces it).
- **Narrow-terminal fallback.** Below ~120 columns the right stack collapses, leaving only the left panel pane visible. A status hint indicates collapsed views. `+`/`-` still cycle through the carousel to inspect hidden views one at a time.
- **Overlays stay overlays.** The picker, validation modal, edit overlay, and help surface remain centered overlays on top of the tiled layout — they do not become panes.

## Capabilities

### New Capabilities
- `tiling-window-manager`: multi-pane tiling in the main band (left panel pane + right view stack), carousel rotation, focus routing, narrow-terminal fallback, split-ratio adjustment.
- `optimizer-pane`: optimizer as a side pane instead of a centered modal, with live preview and candidate comparison while panels remain visible.

### Modified Capabilities
- `keybinding`: **BREAKING** — zoom-like actions consolidated onto `+`/`-` with modifier variants; `[`/`]` now control split ratio (was viewer split only); `\` toggles optional vertical split in left pane; quad-view key (`g q`) removed; optimizer key (`g o`) opens pane instead of modal.
- `viewer-layout`: **BREAKING** — embedded viewer becomes one slot in the right stack instead of a fixed half-screen split; vertical split toggle (`\`) replaces the quad view as the side-by-side mechanism.
- `signal-flow-graph`: graph camera zoom now responds to `+`/`-` when graph pane has focus, `Ctrl++`/`Ctrl+-` regardless of focus; cable tension responds to `Alt+[`/`Alt+]`.
- `visual-validation`: gallery/snapshot coverage for tiled layouts, focus states, narrow-terminal collapse, and optimizer-in-pane rendering.

## Impact

- `src/app.rs` — tiling state (`TileStack`, `FocusSlot`, carousel index, split ratios), focus tracking, `toggle_tile_view`, `cycle_focus`, `adjust_split_ratio`. Removal of quad-view fields (`showing_quad`, `quad_focus`, `filtered_graph`, etc.).
- `src/handler.rs` — key routing rewritten: focus determines which handler branch receives keys; new bindings for carousel (`Shift+Tab`/`Tab`), split toggle (`\`), zoom family (`+`/`-`/`Shift++`/`Shift+-`), split ratio (`[`/`]`), cable tension (`Alt+[`/`Alt+]`); optimizer opens pane instead of modal; quad-view keys removed.
- `src/ui.rs` — tiling renderer: left pane (always panels, optionally split vertically) + right stack (horizontal cuts for open views); focus border rendering; narrow-terminal collapse; optimizer pane rendering; removal of `render_embedded_main` and `render_optimizer_modal`.
- `src/theme.rs` — new tokens `pane_focus_border`, `pane_unfocused_border`; removal of `optimizer_modal_border` (replaced by pane tokens).
- `src/events.rs` — optional `FocusChanged(FocusSlot)` event for future subscribers.
- `openspec/specs/` — new specs for `tiling-window-manager`, `optimizer-pane`; deltas for `keybinding`, `viewer-layout`, `signal-flow-graph`, `visual-validation`.

## Non-Goals

- No floating/draggable panes — slots are fixed, views rotate through them.
- No persistence of layout state across sessions — layout resets on restart (YAGNI).
- No kitty-graphics pane borders or per-pane image rendering — box-drawing borders suffice.
- No herdr or second-process integration — all panes render in-process.
- No per-view independent zoom persistence — zoom resets on view change (existing behavior preserved).
