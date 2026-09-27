# Design

## Context

See proposal.md for motivation. The current state that shapes the approach:

- The main band is painted by `paint_tiled` in `src/gui/mod.rs`. It always paints the left panels pane, then cuts the right column into `slots.len()` equal horizontal slots and dispatches each slot's `ViewType`. The `ViewType::Optimizer` arm is empty and carries a comment that the optimizer renders as an overlay, and `paint_overlays` draws `paint_optimizer` as a card anchored at `Pos2::ZERO`, which lands on top of the panels pane.
- `App.tile_stack` (`TileStack { slots: Vec<ViewType>, focus: FocusSlot }`) is the layout source of truth, with `MAX_TILE_SLOTS` and a `TILE_CAROUSEL`. Legacy `showing_*` booleans mirror it.
- `App.main_split_ratio` (default 0.6, clamped 0.3 to 0.7) holds the left and right ratio and is kept in sync with `viewer_split_ratio`. `App.left_split_ratio` (default 0.5) holds the optional `\` vertical sub-split.
- `App.quad_active` and `QuadFocus` drive the separate 2x2 quad path.
- `App.pane_rects: Vec<(FocusSlot, Rect)>` is published every frame for focus hit-testing, and the window path maps pointer input through `graph_pane_origin`.

## Goals / Non-Goals

**Goals:**
- One layout model with class-tagged panes, replacing both the tiling and the quad path.
- The right half adapts: a second big pane when no small-class view is open, two small panes when one is.
- Class routing decides which pane a view opens in.
- Maximize and same-class swap as keyboard operations on the focused pane.
- The optimizer renders only inside its own pane.

**Non-Goals:**
- No new view types beyond treating the module UI as one.
- No pointer drag to resize panes.
- No layout persistence across restarts.
- No change to how a view paints its own content beyond the rect it is handed.

## Decisions

### Decision 1: A fixed left pane with an adaptive right half

The layout is not a general split tree. It is a left big pane at half width plus a right half that takes one of two shapes:

- No small-class view open: the right half is one big pane, giving two equal panes side by side.
- A small-class view open: the right half splits vertically into two small panes of a quarter each.

Two ratios cover both shapes: the left and right boundary, and the boundary between the two small panes. A general tree would need split nodes and a ratio per node, and nothing in the request needs more than four panes.

Alternative considered: keep the slot vector and add a class tag per slot. Rejected because the target shapes are not expressible as a flat stack, which is what the right column is today.

### Decision 2: The module UI becomes a view in the Big class

The module UI stops being a permanently painted left pane and becomes a `ViewType::Panels` value that occupies a big pane. This follows from the request: the big pane is where the module UI lives at startup, and swap has to be able to move it out.

Consequence: opening the graph into an occupied big pane replaces that pane's view. `Alt+b` from a small pane brings the module UI back into the big slot. This is the case the request describes with the module UI and graph example.

### Decision 3: Physical and module UI never coexist

The two views that draw the same hardware are mutually exclusive. Opening either closes the other, and swap cannot leave both open. Without the rule, the two-big-pane arrangement would let both sit side by side, which the request rules out.

Implementation: enforce it in `open_view`, where a view in the pair closes its partner first. Swap moves existing views only, so it cannot create the pair; the requirement is still asserted by a test.

### Decision 4: Swap semantics

`Alt+b` exchanges the focused pane's view with the big pane that is not focused. In the two-big-pane arrangement that swaps the two big panes. In the small arrangement, focusing a small pane swaps it with the left big pane, and focusing the left big pane is a no-op because it is the only big pane.

`Alt+s` exchanges the two small panes, and is a no-op when the right half is a single big pane.

Alternative considered: rotate views among panes of the same class only. With one big pane that operation is always a no-op, so it cannot express the example the request gives.

### Decision 5: Opening a small view repurposes the right half

When a small-class view opens while the right half holds a big-class view, that big-class view closes and the right half splits. The pane is being repurposed, and parking a hidden view would leave a view the user cannot see or reach.

### Decision 6: Maximize is display state, not a mode

`App.maximized: Option<PaneId>` holds the maximized pane. It clears on `z`, on a focus change, and on `Esc`. Nothing else reads it and it is not persisted, which is what keeps it non-latching.

### Decision 7: View-local key precedence for `[` and `]`

`[` and `]` already mean different things per focused surface: the optimizer pane adjusts the objective weight `w` with them, and the graph pane leaves them alone (cable tension is `Alt+[`/`Alt+]`). The global pane boundary adjustment takes `[`/`]` only when the focused view does not consume them. The optimizer keeps its `w` binding while focused, so the boundary adjustment is unavailable there.

Alternative considered: move the boundary adjustment to a distinct key. Rejected because `[`/`]` is the established split key in this app and the optimizer's `w` binding is already specified.

### Decision 8: Delete the optimizer overlay path

`paint_optimizer` takes a pane rect instead of the full canvas and is called from the pane dispatch. The `paint_overlays` call site for the optimizer goes away, along with the empty `ViewType::Optimizer` arm in the tile painter.

## Risks / Trade-offs

- Uncommitted help-parity work overlaps `src/gui/mod.rs` and `src/gui/overlays.rs` → commit or stash it before implementation starts, so the two edit sets do not collide.
- Removing two capabilities deletes their specs at archive time → the influence highlight and dim styling that quad-view described still exists, and the removal deltas record that it moves to the graph surface rather than disappearing.
- The arrangement changes shape when the first small view opens and when the last one closes, so a view can be closed as a side effect → the status line reports the repurposed pane, and the test suite pins both transitions.
- The module UI becomes closable, so a user can end up with an empty big pane → `Esc` leaves the pane empty and the status line reports the empty state. Reopening is one keypress.
- `Alt` combinations can be swallowed by some desktop environments → the handler routes them through the same neutral `KeyEvent` path as the other modifier chords, and the help table documents them.
