## Context

Today `droid_tui` renders one exclusive surface in its main band: panels, source viewer, signal-flow graph, physical layout, or the optimizer modal. The quad view (`modifier-flow-quad-view`, archived) proved the value of co-visible signals but was rigid and modifier-specific. The current `App` state carries separate booleans for each view (`showing_viewer`, `showing_graph`, `showing_quad`, etc.), and `handler.rs` routes keys through a priority chain that checks each view flag.

The `viewer_split_ratio` field already implements a two-column split (panels | source). The tiling manager generalizes this: the left pane is always panels; the right column is a stack of view slots. The split ratio mechanism extends naturally to any right-column view, not just the source viewer.

See `proposal.md` for motivation and `specs/` for behavior requirements.

## Goals / Non-Goals

**Goals:**
- Replace exclusive-view model with a tiling layout: panels (left) + view stack (right, horizontal cuts)
- Carousel to rotate views through slots; focus routing so keys reach the active pane
- Promote optimizer from modal to side pane
- Consolidate zoom-like actions onto `+`/`-` with modifier variants
- Narrow-terminal fallback below 120 cols
- Remove quad view as a permanent surface; survive as optional vertical split in left pane

**Non-Goals:**
- Floating/draggable panes — slots are fixed
- Persistence of layout state across sessions
- Kitty-graphics pane borders
- Herdr or second-process integration

## Decisions

### D1: TileStack model over per-view booleans

Replace `showing_viewer`, `showing_graph`, `showing_quad`, etc. with a single `TileStack` struct:

```rust
struct TileStack {
    /// Views in the right column, top to bottom.
    slots: Vec<ViewType>,
    /// Currently focused slot index (None = left panel pane focused).
    focus: Option<usize>,
}

enum ViewType { Graph, SourceViewer, Physical, Optimizer }
```

**Rationale:** A single source of truth for "what views are open" eliminates the combinatorial state space of multiple booleans. Adding a new view type requires only one enum variant, not a new bool + new handler branch + new render branch. The `focus` index replaces `ViewerFocus` and generalizes to N panes.

**Alternatives considered:**
- Keep booleans + add a `Vec<ViewType>` overlay — too much redundancy, two sources of truth.
- Bit flags — less readable, harder to iterate for rendering.

### D2: Split ratio generalized from viewer_split_ratio

Rename `viewer_split_ratio` to `main_split_ratio` (f32, default 0.6, clamped 0.3–0.7). It controls the left/right horizontal split regardless of which views occupy the right column. The vertical split within the left pane (quad replacement) gets `left_split_ratio` (f32, default 0.5, clamped 0.3–0.7).

**Rationale:** The existing field already does the right thing; renaming and re-scoping is a one-line change plus updating the keybinding handler. No new layout algorithm needed.

### D3: Carousel replaces quad-view toggle

The quad view (`showing_quad`) is removed. Its functionality — seeing panels + another view side by side — is covered by the default tiled layout (left = panels, right = any view). The `\` key toggles a vertical split inside the left pane for side-by-side comparison, using the same `TileStack` model for the left pane's sub-slots.

**Rationale:** The quad view was a special case of "I want to see two things at once." The tiling model handles this generally: any combination of views can coexist. The `\` toggle covers the specific case where the user wants panels split vertically (e.g., panels + influence subgraph).

**Alternatives considered:**
- Keep quad view as a separate mode — would duplicate layout code and confuse the keybinding model.
- Make quad view the default tiled layout — too rigid; users may want panels-only or panels+graph without source.

### D4: Zoom family consolidated on `+`/`-` with modifiers

- Plain `+`/`-`: scale the focused pane (panels scale factor when panels focused, graph camera zoom when graph focused, source text size when source focused — though source doesn't currently scale, this is the extension point).
- `Shift++`/`Shift+-`: scale the other pane(s), i.e., any pane that is not focused.
- `Alt+[`/`Alt+]`: cable tension (graph-specific, only when graph has focus).

**Rationale:** Today `[`/`]` adjust the viewer split ratio and `+`/`-` adjust panel scale, but they only work in specific contexts (viewer open for split, panels visible for scale). Consolidating onto `+`/`-` means the keys always work — they just affect the focused pane. The modifier variants give access to the other pane(s) without switching focus.

**Alternatives considered:**
- Keep existing keys — users would need to remember which keys work in which context.
- Use number keys for pane-specific zoom — too many keys, conflicts with shift groups.

### D5: Narrow-terminal fallback collapses right column

Below 120 columns, the right column disappears. The carousel inspects hidden views by temporarily replacing the left pane. Status hint shows "+N views hidden".

**Rationale:** Below 120 cols, even two panes at 30/70 split give the right pane only ~36 cols at 120-wide, which is unreadable for source/graph. Collapsing to one pane preserves readability. The carousel gives access to hidden views without permanent layout degradation.

**Threshold:** 120 cols is the point where a 40% right column at 75% zoom still renders panel cells at minimum boxable width (see `DESIGN.md` floor 0.75).

### D6: Optimizer pane reuses existing candidate generation

The optimizer's candidate generation (`optimize.rs::generate_candidates`) and preview logic remain unchanged. Only the rendering path changes: `render_optimizer_modal` is replaced by a pane renderer that slots into the right column. The modal border token is replaced by the pane focus border token.

**Rationale:** The optimizer's core logic is sound; only its presentation changes. Reusing the existing code minimizes risk and keeps the change focused on layout.

### D7: Focus routing in handler.rs

The key priority chain in `handle_event` changes from:
```
overlay → picker → prefix → graph surface → embedded-viewer focus → normal keys
```
to:
```
overlay → picker → prefix → focused pane handler → fallback keys
```

The focused pane handler dispatches based on `TileStack.focus`:
- `None` (panels focused): panel keys (toggle, shift, scale, orientation)
- `Some(idx)` where `slots[idx]` is Graph: graph keys (drag, x, p, zoom, tension)
- `Some(idx)` where `slots[idx]` is SourceViewer: viewer keys (j/k scroll, occurrence nav, t, Tab)
- etc.

**Rationale:** This is a direct generalization of the existing priority chain. Instead of checking `showing_graph` / `showing_viewer` / `showing_quad`, we check the focused slot's view type.

## Risks / Trade-offs

- **[Risk] Breaking change to keybindings.** Users accustomed to `[`/`]` for viewer split and `g q` for quad view will need to learn new bindings. **Mitigation:** The help modal (`?`) updates to show the new bindings. The status bar shows context-sensitive hints.
- **[Risk] Layout complexity with 3+ views.** Three horizontal slots in the right column at 80% split ratio gives each slot ~13% of total width at 120 cols. **Mitigation:** The narrow-terminal fallback (D5) kicks in at 120 cols. Users on wider terminals benefit from the multi-view layout.
- **[Risk] Test migration burden.** Many existing tests assert view-state booleans that no longer exist. **Mitigation:** Tests update to assert `TileStack` state. Snapshot tests regenerate with new layouts.
- **[Trade-off] Carousel replaces focused view vs. inserts new view.** The current design replaces the focused slot's view when cycling. Inserting a new view requires opening via `g g`/`g v`/`s`, which adds a slot. This means `Tab` is for rotating views in an existing slot, not for adding views. **Rationale:** Keeps the carousel predictable — it rotates, it doesn't mutate the slot set. Adding views is an explicit action (`g g`, `g v`, `s`).
