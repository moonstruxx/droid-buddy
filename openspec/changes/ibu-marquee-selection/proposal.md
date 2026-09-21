# Marquee selection commits to shared App selection (droid_tui-ibu)

The GPU graph window already computes a live marquee: an empty-canvas drag
collects the enclosed scene-node indices into `WindowFrame.marquee`, and
`GraphWindow.selected_nodes` highlights them on the canvas. But
`handle_graph_window_frame` never reads `frame.marquee`, so the N-node
selection stays window-local and the spec scenario fails: enclosed nodes
never reach the shared selection and nothing propagates to the viewer,
panels, or terminal tile.

This change wires the marquee into the existing shared selection path.
When a frame carries a non-empty marquee, the handler commits the first
enclosed node (scene order, deterministic) through `App::select_circuit`,
the same call the single-click path uses. That reuses the whole
propagation chain for free: source viewer jumps and opens, panel hardware
highlights, circuit influence recomputes. Re-selecting the same primary is
idempotent, so holding or extending the drag without changing the first
enclosed node causes no state churn. An empty marquee changes nothing, and
the single-click, node-drag, pin, camera-key, and scene-rebuild paths stay
exactly as they are.

Deliberately out of scope: a multi-node model in `App`. `selected_circuit`
stays a single `Option<NodeId>`; the full enclosed set keeps living in the
window-local highlight. Growing a shared multi-select would ripple through
the panels, physical view, viewer, and dependency filter, which is a
separate design decision, not this bug fix. `selected_component` (a hardware
token id) is also untouched: graph nodes are circuits and the
circuit-to-token mapping is ambiguous, while `select_circuit` already drives
the panel hardware highlight through `circuit_hw_token_indices`.
