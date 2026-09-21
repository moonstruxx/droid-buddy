# Spec vs code audit

Audit of the droid_tui behavior specs against the current code. Each spec scenario was checked for whether the code actually does what the spec says.

Date: 2026-09-19.

## How this was done

Two read-only passes compared spec text to the live code:

- One pass covered signal-flow-graph, quad-view, tiling-window-manager, latency-optimizer.
- One pass covered controller-panels, mouse-interaction, module-scaling, module-orientation, modifier-panel-highlight.

Each pass emitted one verdict line per spec scenario: `Spec | Scenario | Result | Evidence`, where the evidence cites the implementing file and line. The code was read at the state of the working tree on the audit date, with no edits made.

Verification was by code inspection, not by running the app. A CONFORMANT verdict means the code path exists and matches the spec text. It does not replace live-screen validation.

## Results at a glance

86 scenarios checked. 63 conform, 23 diverge.

| Spec | Scenarios | Conformant | Diverged |
|---|---|---|---|
| signal-flow-graph | 28 | 24 | 4 |
| quad-view | 6 | 4 | 2 |
| tiling-window-manager | 7 | 3 | 4 |
| latency-optimizer | 10 | 10 | 0 |
| controller-panels | 11 | 7 | 4 |
| mouse-interaction | 11 | 9 | 2 |
| module-scaling | 4 | 0 | 4 |
| module-orientation | 4 | 4 | 0 |
| modifier-panel-highlight | 5 | 2 | 3 |
| Total | 86 | 63 | 23 |

## How to read the divergences

The 23 divergent scenarios fall into three groups. The grouping decides what to do next, and only the third group needs code changes.

### Superseded specs (8)

These scenarios describe a rendering or terminal stack that the project removed. The current behavior is intentional. The specs are stale and should be retired or rewritten.

| Spec | Scenario | What changed |
|---|---|---|
| signal-flow-graph | ComfyUI-style rendering | Spec says cable edges render as box-drawing characters. The box-drawing renderer was deleted by the native-egui move; edges are now bezier curves and arrows in egui (gui/graph.rs:28,133-135). |
| signal-flow-graph | Kitty-graphics graph rendering | No kitty-gfx feature in Cargo.toml, and src/kitty_protocol.rs was deleted. There is no box-drawing fallback either. |
| signal-flow-graph | Interactions preserved on the image renderer | The kitty image renderer was removed. Drag, re-settle, hover, x, e, diff, latency, and topology now live on the egui window path (handler.rs:1914-2007). |
| quad-view | Kitty-gfx optional polish | Feature and KITTY_WINDOW_ID/TERM detection removed. No protocol file, no fallback. |
| quad-view | Visual validation for quad | No insta dev-dependency and no snapshot-gallery bin. Snapshots and gallery were deleted; visual checks are now headless egui assertions. |
| mouse-interaction | Enable mouse capture | The crossterm capture spec is replaced by the winit/egui window. There is no crossterm enable or disable on startup and exit. |
| mouse-interaction | Multiplexer compatibility | Terminal-era Herdr/tmux requirement. The app is now a native desktop window with no multiplexer scope. |
| module-scaling | Component scaling presets | Spec lists a 50% preset. Current presets are [0.75, 1.0, 1.5, 2.0] (app.rs:771, handler.rs:337-340) with the 0.75 floor kept so cells stay boxable. The 50% preset is intentionally unreachable. |

### Behavior mismatches needing a decision (3)

The code and the spec disagree about intended behavior. Pick which side is right, then either change the code or update the spec.

| Spec | Scenario | Code today |
|---|---|---|
| module-scaling | Scale factor persistence across patch loads | `load_patch` resets `physical_zoom` to 1.0 (app.rs:2215, 2292). Only config `[physical] zoom` persists across sessions, not across in-session patch loads. |
| module-scaling | Scale factor is independent of orientation | One `physical_zoom`/`scale_factor` is shared by both orientations. There is no per-orientation scale store. |
| module-scaling | Minimum component size is preserved | The spec's 40x20 px floor does not exist in the egui path. Cells are sized by the mm-to-screen mapping and text clips instead (gui/physical.rs:293-333). |

### Genuine code gaps (12)

The spec describes behavior the code does not implement. This is the fix list.

| Spec | Scenario | Code today | Fix direction |
|---|---|---|---|
| controller-panels | Controller panel component labels use overlay fallback | panels.rs:232,293 and physical.rs:947 set the cell label from `comp.label.clone()`. `Patch::display_label` is never called for HW cells; only `circuit_display_label` is used (viewer.rs:992,1026). Edited labels never render on the panels or physical surface. | Resolve HW-cell labels through `Patch::display_label(token, effective_shift, layers_enabled, max_shift_layer, &store)`. |
| tiling-window-manager | Focus routing | One focused pane and Tab cycling work, but the focus border renders only on panels via the `focus_border` token (panels.rs:71-72). `pane_focus_border` and `pane_unfocused_border` exist in theme.rs and are used by no renderer (gui grep: 0 hits). Graph and source pane clicks do not set tile focus. | Draw pane frames with `pane_focus_border`/`pane_unfocused_border` on every pane, and set focus on pane click. |
| modifier-panel-highlight | Momentary hold highlight | The wash paints only on the held cell (gui/physical.rs:271-279, panels.rs:251), not the influence set. Unaffected cells are not dimmed. No MOD status string exists anywhere. | Highlight the influence set with the per-token `modifier_hue`, dim the rest, and add the `MOD <token> -> N cells / M cables` status. |
| modifier-panel-highlight | Latched additive highlight via Ctrl+Shift+Click | No latch state exists. Ctrl+Shift+Click / Ctrl+Click toggle and the `m` alias are not implemented; hold only happens when modifiers equal NONE (handler.rs:1648-1650). | Add latch state so a Ctrl+Shift+Click, Ctrl+Click, or `m` on a modifier-eligible component latches on; a further press or Esc clears. Keep momentary hold. Single-var is fine; additive union is aspirational. |
| modifier-panel-highlight | Cross-view hue parity and shift coexistence | Graph influence uses the fixed `graph_node_highlight` token (gui/graph.rs:853-865), not the per-token hue. The source viewer has no modifier-hue select spans. The MOD status hint is absent, and the wash is limited to the held cell. | Use `modifier_hue` in the graph influence highlight and add hue select spans in the viewer, matching the panels surface. |
| controller-panels | Module-aware layout calculation | panels.rs:188-201 sets block width to `comps.len() * uniform PANEL_CELL_W`, so panels size by component count, not HP module width. Variable-width modules exist only in the physical 1:1 view. | Size panel blocks by module HP width rather than component count. |
| controller-panels | Box LED-associated elements | LED parsing exists (patch.rs:766-888; led_pairs.ini, numbered_led_pairs.ini), but the window renderer has no boxed path. `paint_cell` (gui/physical.rs:242-333) paints text cells only, and `CellSpec` has no border or LED-glyph fields. | Add the boxed cell rendering path for LED-associated elements. |
| controller-panels | Box geometry and hit-testing | The renderer-owns-geometry contract holds for text cells, but no boxed cells render, so box hit rects and LED-in-box glyph updates cannot match. | Same as above; publish box hit rects. |
| tiling-window-manager | Tiled main band layout | Left panels plus right-column cuts and the 3-slot cap with oldest-non-focused eviction both conform (gui/mod.rs:641-696, app.rs:534,1449). But `s` toggles the skeleton presentation instead of opening a Physical slot (handler.rs:1358-1368), and a Physical slot paints nothing (gui/mod.rs:693). | Make `s` open or focus a Physical slot, and give the Physical view a paint path. |
| tiling-window-manager | Carousel rotation | `TILE_CAROUSEL` and `cycle_view_in_slot` exist (app.rs:536-538,1532), but no key is bound to them. Tab is bound to `cycle_focus` (handler.rs:738-748) and never replaces a slot's view, so the "Tab cycles to next view" scenario fails. | Bind a carousel key so Tab (or a distinct key) rotates the focused slot's view. |
| tiling-window-manager | Vertical split toggle (quad replacement) | `\` toggles the left split (handler.rs:1456, app.rs:1573), but the quad `g q` still exists and is the quad mechanism (handler.rs:606-613, paint_quad), so it was not replaced. Alt+[ and Alt+] adjust cable tension, never `left_split_ratio` (handler.rs:1044-1060). | Point Alt+[ / Alt+] at `left_split_ratio`, and retire or gate the quad path. |
| signal-flow-graph | Cable tension keys | Alt+[ and Alt+] step `TENSION_STEP=0.05` and re-solve (layout.rs:51, app.rs:2873-2882), but `adjust_tension` shows status and rebuilds on the column arrangement too. handler.rs:1050-1060 has no mode gate, so it violates the spec's "no status is shown on column" clause. | Gate the tension status and rebuild on force mode, or update the spec. |

## Full verdict tables

The complete per-scenario results, kept for traceability.

### signal-flow-graph

| Scenario | Result | Evidence |
|---|---|---|
| Virtual-cable extraction | CONFORMANT | src/graph.rs build_from_patch cable extraction (catalog output->source, input->sink, unknown-circuit output convention, n->1 flag) plus graph.rs tests |
| Banner-range grouping | CONFORMANT | app.rs clusters_from_patch; gui/graph.rs:65-74 cluster containers; layout.rs:64-65 cohesion force toward banner centroid |
| Convergence-based layout | CONFORMANT | layout.rs:123 solve bounded energy-threshold freeze; app.rs:3383 seed_tip_pin; drag->local_resettle (handler.rs:1996-2005); column is default (main.rs seed_app Column/Strict), force via `h` |
| ComfyUI-style rendering | DIVERGED | Rounded frames, title, ports, colors, and clusters render in egui (gui/graph.rs:28,133-135), but cable edges are bezier curves and arrows, not box-drawing characters. The box-drawing renderer was deleted by native-egui-rendering. |
| Edge color from cable kind | CONFORMANT | graph_render.rs SceneSpec edge color from declared `cable_kind`, substring-inference fallback; graph.rs tests |
| Node color for plugin circuits | CONFORMANT | graph.rs node color from declared `color` token, inference fallback |
| View-switch key | CONFORMANT | handler.rs:555-575 `g g` opens the graph slot (focus_tile_slot); Esc closes via close_focused_view (handler.rs:749-763, app.rs:1494) |
| FULL graph highlights influenced path | CONFORMANT | recompute_influence plus scene highlight/dim tokens (app.rs; gui/graph.rs highlight/dim rendering) |
| FILTERED graph is independently solved | CONFORMANT | InfluenceSubset own solve (app.rs:463-473); gui/mod.rs:627-632 fresh compact fit per pane |
| Graph reflects disabled circuits | CONFORMANT | `x` toggle processing (handler.rs:990-995), dimmed node and edges on rebuild, influence yields to disabled |
| Graph node titles use circuit label override | CONFORMANT | label_store.circuit_label feeds node titles; regression.rs:758 visual_diff_changed_node_marker_snapshot |
| Latency optimizer menu (g o) | CONFORMANT | handler.rs:588-597 `g o`; app.rs:1237-1267 open_optimizer (no-patch hint, at most 3 candidates); j/k/Enter/s/r/Esc; preview recolors and status label (app.rs:1356-1368) |
| Kitty-graphics graph rendering | DIVERGED | No kitty-gfx feature in Cargo.toml; src/kitty_protocol.rs was deleted by native-egui-rendering (8d0e33b); no box-drawing fallback either |
| Pan and zoom navigation | CONFORMANT | `+`/`-` presets plus arrows pan (handler.rs:1029-1090); initial fit vs pane (fit_graph_camera, app.rs:2095); presets extend at least 2x below fit (0.03125, GRAPH_ZOOM_PRESETS); pane-center anchored |
| Center and fit-and-center keys | CONFORMANT | `c` -> center_graph_camera, `Shift+c` -> fit_graph_camera (handler.rs:972-989); no-op without a graph; help.rs:95-96 documents both, test keybindings_reflect_graph_center_fit_and_g_c_chord |
| Latency coloring toggle on the g-prefix | CONFORMANT | `g c` -> toggle_latency_coloring (handler.rs:598-605); bare `c` centers only |
| Interactions preserved on the image renderer | DIVERGED | Kit kitty image renderer removed; interactions (drag/re-settle/NodeMoved, hover, x, e, diff/latency/topology) now live on the egui window path (handler.rs:1914-2007) |
| Theme colors map to RGB | CONFORMANT | gui/graph.rs rgb() derives all colors from theme tokens; error > diff > ramp > kind precedence preserved in build_scene |
| Graph camera zoom via focus | CONFORMANT | `+`/`-` zoom the camera when the graph slot is focused; panels-focused `Shift++` zooms the graph camera (handler.rs:1037-1042, both scenarios pass) |
| Cable tension keys | DIVERGED | Alt+[ / Alt+] step TENSION_STEP=0.05 and re-solve (layout.rs:51, app.rs:2873-2882), but adjust_tension shows status and rebuilds on the column arrangement too; handler.rs:1050-1060 has no mode gate, violating "no status is shown" on column |
| Manual pin and drag-to-place | CONFORMANT | `p` toggle (handler.rs:961-971); drag auto-pins at drop (app.rs:814 doc); tip pinned by default (seed_tip_pin app.rs:3383); column anchors pinned nodes (solve_graph_positions app.rs:2659-2677) |
| GPU window surface | CONFORMANT | GraphWindow (winit/egui/wgpu, gui/mod.rs); same model, positions, and camera; x/p/e/diff/latency/topology via WindowGraphKey plus handle_graph_window_frame |
| Circuit selection shared across surfaces | CONFORMANT | Window node click -> select_circuit (handler.rs:1954-1956); terminal tile same path; panels and source reflect shared selection (marquee multi-select gap tracked separately: bd droid_tui-ibu) |
| Register and hardware nodes | CONFORMANT | Controller and jack nodes, register edges, dedup, unmatched-controller to jack, NotSelected edge drop (graph.rs; register-jack-controller-nodes change) |
| Select-state filtering | CONFORMANT | `g s` menu (handler.rs:625-632), open_select_menu and cycle_select_candidate (app.rs:1748/1786), expression evaluation, reset on patch load |
| Upstream dependency subgraph | CONFORMANT | toggle_dependency_filter (app.rs:2888-2913), BFS stopping at controller and jack, root frozen, clear paths, "Dependencies of X: N nodes", presentation-only |
| Column layout arrangement | CONFORMANT | solve_columns_pinned dense-normalized, width-aware, grid snapped, outer controller and jack columns, deterministic, default, `h` toggle (app.rs:2659-2677, layout.rs) |
| Configurable within-column ordering | CONFORMANT | config [layout] ordering Strict/Barycenter (config.rs LayoutOrdering); solve_columns_pinned ordering param |

### quad-view

| Scenario | Result | Evidence |
|---|---|---|
| Quad concurrent layout | CONFORMANT | paint_quad 2x2 (gui/mod.rs:590+), quad_rects via main_split_ratio/left_split_ratio; <120px fallback (gui/mod.rs:848, QUAD_WIDTH_THRESHOLD app.rs:461); Tab focusable |
| Highlight vs dim in FULL graph | CONFORMANT | Influence set drives graph_edge_highlight/graph_edge_dim plus node tokens (recompute_influence, build_scene) |
| Filtered graph re-solves compactly for readability | CONFORMANT | InfluenceSubset independent solve; gui/mod.rs:627-632 fresh fit with its own compact camera |
| Focus cycle and keys in quad mode | CONFORMANT | cycle_quad_focus ORDER Panels->Source->GraphFull->GraphFiltered (app.rs:1631-1660); Tab/Shift+Tab (handler.rs:718-735); Esc -> exit_quad keeps selection; drag -> local_resettle plus NodeMoved |
| Kitty-gfx optional polish | DIVERGED | kitty-gfx feature and KITTY_WINDOW_ID/TERM detection removed; no kitty_protocol.rs, no box-drawing fallback (Cargo.toml, native-egui-rendering) |
| Visual validation for quad | DIVERGED | No insta dev-dep and no snapshot-gallery bin (Cargo.toml); insta snapshots and gallery deleted (4989920; ca22cb2 "no insta snapshots exist") |

### tiling-window-manager

| Scenario | Result | Evidence |
|---|---|---|
| Tiled main band layout | DIVERGED | Left panels plus right-column horizontal cuts, 3-slot cap, and 4th-evicts-oldest-non-focused all conform (gui/mod.rs:641-696, app.rs:534,1449), but `s` toggles the skeleton presentation instead of opening a Physical slot (handler.rs:1358-1368), and a Physical slot paints nothing (gui/mod.rs:693) |
| Carousel rotation | DIVERGED | TILE_CAROUSEL and cycle_view_in_slot exist (app.rs:536-538,1532), but no keybinding; Tab is bound to cycle_focus (handler.rs:738-748) and never replaces a slot's view, so "Tab cycles to next view" fails |
| Focus routing | DIVERGED | One focused pane, Tab cycle, and keys route by focus conform, but the focus border renders only on panels via the `focus_border` token (panels.rs:71-72), pane_focus_border/pane_unfocused_border are unused by any renderer (gui grep: 0 hits), and graph and source pane clicks do not set tile focus |
| Split ratio adjustment | CONFORMANT | Default 0.6 (app.rs:940), `[`/`]` 0.1 steps clamped [0.3,0.7] (handler.rs:1061-1066, app.rs:3397-3400), persists across views, resets on restart |
| Vertical split toggle (quad replacement) | DIVERGED | `\` toggles the left split (handler.rs:1456, app.rs:1573), but quad `g q` still exists and is the quad mechanism (handler.rs:606-613, paint_quad), so it was not replaced; Alt+[ / Alt+] adjust cable tension, never left_split_ratio (handler.rs:1044-1060) |
| Esc closes focused view | CONFORMANT | handler.rs:749-763 -> close_focused_view (app.rs:1481-1503); graph, viewer, optimizer, and physical close plus slot removal; panels Esc clears selection |
| Picker and overlays stay on top | CONFORMANT | paint_overlays after the base surface (gui/mod.rs:702-728): diff, optimizer, validation, select, picker, label edit, help |

### latency-optimizer

| Scenario | Result | Evidence |
|---|---|---|
| Candidate generation (up to 3) | CONFORMANT | generate_candidates_weighted_fast plus OptimizeScope::MinMax 3 strategies (optimize.rs:88-96); FNV-1a seeded no RNG (optimize.rs:425); CandidateOrdering{label,order,summary} (optimize.rs:72); objective from latency.rs forward_latency |
| FAS-indegree first-phase ranking | CONFORMANT | fas_indegree_seed Kahn-style deterministic, runs before every variant (optimize.rs:496) |
| Multilevel coarsening and VNS | CONFORMANT | coarsen_by_banner (optimize.rs:814), search_vns/search_vns_with_budget (880/889); tests coarsen_by_banner_contracts_groups_and_drops_intra_edges and vns_preserves_banner_scope_and_bounded |
| Weighted slider objective | CONFORMANT | Objective::Weighted(w) blended (1-w)*Sum + w*max, w=0 MinSum and w=1 MinMax (optimize.rs:107-124); menu `[`/`]` 0.1 snap plus regenerate (app.rs:1273-1311) |
| SA with seeded PRNG | CONFORMANT | Strategy::Annealing (optimize.rs:133), annealing_seed (982); tests annealing_seeded_determinism and annealing_preserves_same_name_and_banner_scope |
| Same-name relative-order preservation | CONFORMANT | Domain constraints preserve instance order (optimize.rs domains); brute-force equivalence and constraint tests |
| Banner-scoped default | CONFORMANT | OptimizeScope::Banner default (optimize.rs:91); per-banner-group domains, global only as an explicit variant |
| Configurable per-circuit cost model | CONFORMANT | CostModel::from_config (main.rs:40), [latency] per_circuit overrides, shared provider for coloring and optimizer |
| In-memory preview | CONFORMANT | optimizer_preview reorders sections plus rebuild_graph -> Event::GraphRebuilt (app.rs:1356-1368, 2743-2750); restore without reload via inverse_order (app.rs:1337-1351) |
| Save-as export | CONFORMANT | optimizer_export writes `<stem>-latopt.ini` via write_to_ini unique_dest auto-suffix (app.rs:1388+, patch.rs:643-646), atomic tmp+rename, never touches source, status confirms path |

### controller-panels

| Scenario | Result | Evidence |
|---|---|---|
| Group components by controller type | CONFORMANT | src/gui/panels.rs:171-181 groups by comp.controller in declaration order; patch.rs controller_types/pinned_panels assignment |
| Render controller panel with border and title | CONFORMANT | src/gui/panels.rs:192-207 ModuleSpec{rect,title} titled faceplate blocks per controller; pane title " Panels " |
| Position components in physical layout order | CONFORMANT | Physical 1:1 mm grid src/physical.rs PhysicalLayout/ScreenMapping; gui/physical.rs renders real faceplate cells, side-by-side faceplates |
| Display component labels and state | CONFORMANT | gui/physical.rs:293-312 glyph plus label row 1 and state row 2; clip_label ellipsis src/gui/physical.rs:674-686 preserves cell geometry |
| Handle overflow with scrolling or wrapping | CONFORMANT | app.rs:1934 physical_pan_if_overflow; handler.rs:1693/1707 wheel pans an overflowing rack, module geometry intact |
| Support terminal resize | CONFORMANT | Layout recomputed from pane Rect every frame (panels_spec/physical_spec build from the current Rect); no resize-state carry |
| Module-aware layout calculation | DIVERGED | panels.rs:188-201 sets block width to comps.len() times uniform PANEL_CELL_W (fixed component counts, not HP module widths); variable-width modules exist only in the physical 1:1 view, not the panels surface |
| Box LED-associated elements | DIVERGED | LED and ledN parsing present (patch.rs:766-888; led_pairs.ini, numbered_led_pairs.ini), but the window renderer has no boxed path: paint_cell (gui/physical.rs:242-333) paints text cells only, and CellSpec has no border or LED-glyph fields |
| Box geometry and hit-testing | DIVERGED | Renderer-owns-geometry contract holds for text cells, but no boxed cells render, so box hit rects and LED-in-box glyph updates cannot match |
| Panels render dim while processing paused | CONFORMANT | gui/physical.rs:348 dim(rgb, paused); app.rs toggle_processing_pause "Processing paused (p to resume)"; PROCESSING PAUSED marker; geometry unchanged |
| Controller panel component labels use overlay fallback | DIVERGED | panels.rs:232/293 and physical.rs:947 use comp.label.clone(); Patch::display_label is never called for HW cells (only circuit_display_label in viewer.rs:992/1026) |

### mouse-interaction

| Scenario | Result | Evidence |
|---|---|---|
| Enable mouse capture | DIVERGED | Terminal-era crossterm capture spec replaced by the native winit/egui window (src/main.rs); no crossterm enable or disable on startup and exit |
| Click to toggle button state | CONFORMANT | handler.rs:1639-1641 toggle_component on MouseEventKind::Down(Left) on the hit rect |
| Hover highlight | CONFORMANT | handler.rs:1625-1627 hovered_component on Moved; hover distinct from selection |
| Scroll to adjust knob/fader values | CONFORMANT | handler.rs:1695-1718 wheel ScrollUp/ScrollDown adjust_value 0.05 over hovered component |
| Multiplexer compatibility | DIVERGED | Herdr/tmux terminal-era requirement; the app is a native desktop window with no multiplexer scope |
| Keyboard navigation preserved | CONFORMANT | handler.rs full key dispatch (j/k, Enter/Space, 1-4, Esc, q) coexists with mouse paths |
| Click selects component | CONFORMANT | handler.rs:1651 select_component(token) on click; hover (hovered_component) stays distinct from selection |
| Click empty panel space clears selection | CONFORMANT | handler.rs:1678-1680 clear_selected_component without moving source_scroll; a component click moves selection without clear-then-set (1628-1652) |
| Minimap click scrolls source | CONFORMANT | handler.rs:1576-1616 proportional minimap click -> source_scroll, gated on showing_viewer plus minimap_rect |
| Status bar composes each segment exactly once | CONFORMANT | Single status_message composition point gui/viewer.rs:1199-1206; scale and orientation composed once in handler.rs:1439 |
| Wheel pans the physical view | CONFORMANT | handler.rs:1693/1707 physical_pan_if_overflow gates wheel pan; component_rects follow the pan offset |

### module-scaling

| Scenario | Result | Evidence |
|---|---|---|
| Component scaling presets | DIVERGED | `+`/`-` cycle [0.75,1.0,1.5,2.0] (app.rs:771, handler.rs:337-340) as whole-rack uniform zoom with "Scaling: N%" status, but the 50% preset is unreachable (floor 0.75 keeps cells boxable) |
| Scale factor persistence across patch loads | DIVERGED | load_patch resets physical_zoom to 1.0 (app.rs:2215/2292); only config [physical] zoom persists across sessions, not across in-session patch loads |
| Scale factor is independent of orientation | DIVERGED | Single physical_zoom/scale_factor shared by both orientations; no per-orientation scale store |
| Minimum component size is preserved | DIVERGED | No 40x20 px floor in the egui path; cells are sized by the mm-to-screen mapping and text clips instead (gui/physical.rs:293-333) |

### module-orientation

| Scenario | Result | Evidence |
|---|---|---|
| Orientation switching | CONFORMANT | handler.rs:1434-1440 `o` toggles Orientation; status "Scale: {:.1} | Orientation: {:?}" (Portrait/Landscape) |
| Orientation state persists across patch loads | CONFORMANT | Orientation is not in the load_patch reset list (app.rs:2174-2245), survives patch switch |
| Component reflow respects orientation | CONFORMANT | panels.rs:190-238 landscape flows module blocks in horizontal rows with module-level wrap; portrait wraps left to right |
| Default orientation is portrait | CONFORMANT | app.rs:938 orientation: Orientation::Portrait |

### modifier-panel-highlight

| Scenario | Result | Evidence |
|---|---|---|
| Structural influence set per hardware token | CONFORMANT | patch.rs:1094 influence_subtree forward BFS (cycle-safe, deterministic); hw_token_to_vars; app.rs:3211-3229 recompute_influence; test influence_fixture_modifier_switch_passthrough patch.rs:4273 |
| Stable per-modifier hue | CONFORMANT | theme.rs:610 modifier_hue hash(token) % 16, deterministic per run and patch; distinct from graph_edge_error red |
| Momentary hold highlight | DIVERGED | Wash painted only on the held cell (gui/physical.rs:271-279, panels.rs:251), not the influence set; no dimming of unaffected cells; no "MOD B1.1 -> 0 cells" status (no MOD status string anywhere) |
| Latched additive highlight via Ctrl+Shift+Click | DIVERGED | No latch state exists; Ctrl+Shift+Click / Ctrl+Click toggle and the `m` alias are not implemented (hold only when modifiers equal NONE, handler.rs:1648-1650) |
| Cross-view hue parity and shift coexistence | DIVERGED | Graph influence uses the fixed graph_node_highlight token (gui/graph.rs:853-865), not the per-token hue; the source viewer has no modifier-hue select spans; the MOD status hint is absent; the wash is limited to the held cell |

## Method notes and limits

- Verdicts come from reading code, not from running the app. A CONFORMANT result means the code path exists and matches the spec text.
- Line numbers reflect the working tree on 2026-09-19 and will drift.
- The audit covered the nine specs listed above. Other specs in openspec/specs were not checked.
- Some specs describe terminal-era behavior (box-drawing, kitty graphics, crossterm capture, multiplexer support) that the native-egui move retired. Those are grouped as superseded rather than as bugs.
