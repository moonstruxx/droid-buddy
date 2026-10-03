# Design

## Context

See `proposal.md` — Why. Three pieces of existing machinery shape the design:

1. `Patch::display_label` (HW chain: store[layer] → store[1] → preamble[1] → derived) and `Patch::circuit_display_label` (store → raw circuit name) are the only resolution paths; both are pure and store-driven.
2. `Graph::upstream_dependencies(root)` (src/graph.rs) is already the walk the guessing needs: reversed-edge BFS, cycle-safe, deterministic (sorted sources), terminating at controller/input-jack leaves.
3. Label painting: graph node titles are fitted to their frame by shrinking the font, then ellipsized, then dropped below a legibility threshold (ADR 40); module-UI `paint_cell` sizes the label font from cell height (`height × 0.42`, clamped 6–15 pt) and joins it to the glyph row only when the cell is ≥ 28 pt wide.

Two constraints from the project memory apply: the scene build is a per-frame hot path (melody2 graph-open freeze — never run an upstream walk per frame), and every UI change needs live-screen visual proof beyond the test suite.

## Goals / Non-Goals

**Goals:**
- Unlabeled circuits display the nearest meaningful explicit label from their signal-flow ancestry.
- Label size is decoupled from zoom on both surfaces; labels stop shrinking and stop vanishing before they must.
- Everything user-visible stays deterministic and the resolution chain stays one ordered fallback.

**Non-Goals:** listed in `proposal.md` (no global store, no persisted guesses, no edge labels, no size config, no overlay UX change).

## Decisions

### 1. Guess = explicit-only, nearest-first BFS

**Decision:** `Graph::guess_labels(&self, patch) -> HashMap<NodeId, String>` walks `upstream_dependencies` per circuit node (root excluded) and returns the first explicit label: for a circuit node its `labels.toml` circuit store entry; for a controller/jack node its stored HW label (effective layer) or its preamble label. Stored circuit overrides are checked before consulting the map, so the chain is store → guess → raw name everywhere `circuit_display_label` resolves today.

**Why:** explicit-only keeps guesses meaningful — accepting derived labels would make every connected circuit resolve to `Button B3.17` at the leaf, drowning the raw name for no information gain. Nearest-first BFS is deterministic because edges are sorted `(cable, source, sink)` and `upstream_dependencies` already sorts its sources.

**Alternatives considered:** *derived labels count* (always finds something, loses meaning); *all-paths agreement* (needs a label on every branch — far too strict for real patches); *manual suggestion in the `e` overlay only* (never helps passive graph reading, which is where raw names hurt).

### 2. Cache the guess per graph build, never per frame

**Decision:** the map is computed in `open_graph` / `rebuild_graph` and recomputed in `save_edit` (a new upstream label changes downstream guesses), stored as `App.guessed_labels`. The scene build, source viewer, and status paths read only the cache. One-shot cost is `O(n · (n + e))` over the BFS — milliseconds at melody2's 642 nodes — against a hard rule of zero graph walks per repaint.

**Why:** the graph freeze lesson (memory #500) forbids rebuilding indexes on the paint path; label edits are rare so recompute-on-save is cheap.

### 3. No graph, no guess

**Decision:** before the first `g g` the graph is not built (deferred by design), so labels resolve store → raw name; the guess joins the chain only once `App.graph` exists.

**Why:** building the graph at load would reintroduce the ~0.5 s topology-validation cost at startup for a display nicety. The source viewer shows the graph's labels once the graph exists; before that it shows the chain it always showed.

### 4. Screen-space font + B3 fit policy

**Decision:** one fixed base font size per surface (points, egui logical size — independent of both the graph camera and `physical_zoom`): graph node labels use a `NODE_LABEL_SIZE` constant, module-UI cell labels a `CELL_LABEL_SIZE` constant. Rendering: draw at the fixed size, ellipsize to the frame/cell width, hide below about one character of width. The existing hover tooltip and status bar remain the full-label fallback. Port markers, cluster titles, and all chrome keep their current zoom-proportional scaling and thresholds.

**Why:** option B was the user's pick; B3 (of B1/B2/B3) was approved via annotation. Fixed size means no font ever shrinks with zoom; ellipsize-to-frame keeps text inside its node; the one-character hide prevents unreadable slivers; tooltip/status already guarantee the full label is reachable.

**Alternatives considered:** *B1 pure ellipsize* (unreadable one-char dots); *B2 fit-or-hide* (labels disappear earlier than today — the shrink step was the only reason they survived mid zoom); *screen-space with growth at zoom-in* (labels become chrome-size at high zoom — not "its own level", its own level plus a curve).

### 5. Physical mirror

**Decision:** `paint_cell` switches from `height × 0.42` (6–15 pt clamp) to the fixed `CELL_LABEL_SIZE`, keeps the ellipsize-to-cell behavior, and hides below one character of that fixed font (replacing the old magic ≥ 28 pt width gate); the font no longer scales with `physical_zoom`.

**Why:** the user asked for labels at their own level in both views; the same B3 policy keeps the two surfaces consistent.
