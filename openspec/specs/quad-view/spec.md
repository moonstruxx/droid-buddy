# quad-view Specification

## Purpose
Four co-visible panes for modifier-centric debugging: top row panels (modifier selection) + source pane (select-site highlights), bottom row graph FULL (highlighted influence inside full topology) + graph FILTERED (compact isolated subtree).

## Requirements

### Requirement: Quad concurrent layout

The system SHALL render four panes concurrently in one terminal when quad view is active: top row `panels (hw components grouped by controller) | source (raw/prettified, select highlights via modifier_index)`, bottom row `graph FULL (all nodes/edges, influenced path bold/highlight, rest dimmed) | graph FILTERED (only influenced subgraph, freshly converged compact layout)`. The layout extends `viewer_split_ratio` (horizontal) and the existing `header/main/status` vertical split; below ~120 columns it falls back gracefully to the prior exclusive modes (single graph or embedded source-only) so narrow terminals remain usable.

#### Scenario: Quad visible concurrently

- **WHEN** a patch is loaded and a modifier (e.g. `B1.1 -> _CLOCK`) is selected while quad view is open
- **THEN** all four panes are visible at once with correct content in each, focusable via `Tab`, without overlap or clipping at 120 cols.

#### Scenario: Responsive fallback

- **WHEN** the terminal is narrower than the quad threshold (e.g. 80 cols)
- **THEN** the layout collapses to a simpler arrangement (e.g. single graph or panels+source) rather than rendering unreadable 20-col panes.

### Requirement: Highlight vs dim in FULL graph

The system SHALL render the FULL graph with influenced nodes/edges in highlight style (bold + `graph_edge_highlight`/`graph_node_highlight` tokens) and uninfluenced elements dimmed (`graph_edge_dim`/`graph_node_dim` or dim modifier). Color in `mono`/`terminal` palettes remains pairwise distinct per existing design-system guarantees.

#### Scenario: FULL highlight

- **WHEN** the influence set marks `_CLOCK` and its downstream hop edges as influenced
- **THEN** the FULL graph draws those edges/nodes highlighted and the rest dimmed; the influenced path is visually distinct at a glance.

### Requirement: Filtered graph re-solves compactly for readability

The system SHALL freshly `solve()` the filtered induced subgraph on its own (compact bounding-box fit, not reused FULL positions), because readability of the isolated subtree takes priority over stability with FULL positions.

#### Scenario: Filtered re-layout

- **WHEN** the filtered graph contains 3 nodes
- **THEN** it converges to a compact arrangement centered in its pane, not a sparse subset of FULL coordinates.

### Requirement: Focus cycle and keys in quad mode

The system SHALL support `Tab` cycling focus across the four panes (`Panels -> Source -> GraphFull -> GraphFiltered -> Panels ...`), `Esc` closing quad and returning to controller panels with selection preserved, and existing `g g` / `g v` semantics preserved. The file picker remains highest priority. Graph drag in either graph pane triggers `local_resettle` + `NodeMoved`.

#### Scenario: Tab cycle

- **WHEN** quad view is open and the user presses `Tab` repeatedly
- **THEN** focus moves through Panels, Source, Graph FULL, Graph FILTERED in order, and the focused pane's border/title indicates focus.

### Requirement: Kitty-gfx optional polish

Kitty inline-image rendering, if enabled via feature-flag `kitty-gfx`, SHALL be attempted only when `KITTY_WINDOW_ID` or `TERM == xterm-kitty` is detected; otherwise box-drawing rendering is used. No subprocess, no IPC, no Herdr pane in this change.

#### Scenario: Kitty fallback

- **WHEN** the `kitty-gfx` feature is enabled but the terminal is not kitty
- **THEN** the graph panes render with the existing box-drawing edges and rounded node frames — identical to non-kitty behavior.

### Requirement: Visual validation for quad

Gallery/snapshot coverage SHALL include quad-view frames × themes (`classic`/`mono`/`terminal`) × widths and highlight/filtered states for at least the `modifier_switch_passthrough` and existing `led_pairs` fixtures.

#### Scenario: Gallery row per quad state

- **WHEN** `cargo insta test --check` and `cargo run --bin snapshot-gallery` run
- **THEN** the HTML gallery shows rows for quad 4-pane states with correct columns per theme, and CI fails on any snapshot mismatch.
