# Design

## Context

See proposal.md (Why). Current state: `optimizer_preview` (app.rs:1408) reorders sections, calls `rebuild_graph` (re-solves via `solve_graph_positions` dispatching on `layout_mode`/`layout_ordering`), then refits only `if graph_canvas_px.is_some()`. `solve_graph_positions` anchors pins by new index into old positions, which scrambles anchors when node order changes. Graph keys (`h`, tension, `c`/`Shift+c`) already follow a mutate → `rebuild_graph` → refit/center + status pattern to reuse.

## Goals / Non-Goals

**Goals:**

- Reorder-safe anchors; unconditional preview refit against the live pane rect; one graph-pane `a` key applying + cycling arrangements with status feedback.

**Non-Goals:**

- No new solver math; no drag/pin/settle changes; no global binding (see proposal Non-goals).

## Decisions

- **Anchor fix by NodeId carry-over** (over index-based): map each pinned `NodeId` to its old position via the previous graph's node list before solving, then pass `(new_index, old_pos)` anchors. Rationale: only identity survives a reorder; index does not. Alternative (drop all anchors on reorder) rejected — it would visibly jump pinned user placements.
- **Refit against the live pane rect** (over last-published canvas): preview resolves the graph pane's current rect via `pane_geometry` (ADR 35 single source) and fits to it, falling back to the published size only when no graph pane is open. Rationale: fixes the stale-viewport + `None`-skip in one move; mirrors how `graph_pane_rect` already derives from `pane_geometry`.
- **Cycle state on `App` reusing `layout_mode`/`layout_ordering`** (over a parallel enum): `a` steps `Column-Strict → Column-Barycenter → Force`, writing back to the existing fields so `h`, config seeding, and `solve_graph_positions` dispatch keep working unchanged. Rationale: zero new sources of truth; `h` remains a two-state shortcut, `a` is the full cycle.
- **Graph-pane-only `a`** (over global): matches `h`/`c` routing and the user's scope call; optimizer-pane users Tab to the graph or preview's auto-refit covers them.

## Risks / Trade-offs

- [Risk] Live-rect derivation inside `App` couples it to pane geometry → Mitigation: reuse the existing `graph_pane_rect`/`pane_geometry` helper, no new geometry code.
- [Risk] Cycling changes `layout_ordering` as a side effect, surprising `h` users → Mitigation: status line names the active arrangement on every press; `h` still toggles mode only.
- [Risk] Force solve on huge patches is slower than column → Mitigation: bounded one-shot solver already gates this; cycle order puts the cheap column paths first.
