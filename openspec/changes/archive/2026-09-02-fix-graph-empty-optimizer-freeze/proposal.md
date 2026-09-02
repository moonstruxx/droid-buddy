# Change: fix-graph-empty-optimizer-freeze

## Why

Two regressions block core surfaces:

- **Optimizer freeze (droid_tui-hj8, P1)**: opening the optimizer (`g o`) runs `generate_candidates_weighted` synchronously on the main event loop with up to 3 strategies × 2000 evaluations (6000 `forward_latency` hash-map builds + clones) plus `rebuild_graph` on preview. On realistic patches this blocks the TUI for hundreds of milliseconds to seconds, freezing input and appearing as CPU load.
- **Graph empty (droid_tui-fxt, P1)**: opening the graph surface (`g g`) shows a blank screen instead of nodes/edges. The kitty image path succeeds but can emit a visually empty frame (degenerate camera, failed font, or off-viewport rects) and early-returns, skipping the box-drawing fallback. In box-drawing, `graph_fit_camera` can produce a negative `pixel_size` on small areas, clamped to a degenerate camera that places nodes off-screen with zero-size rects, also rendering as empty.

Both are P1 and related to view-surface liveness.

## What Changes

- **Optimizer responsiveness**: make candidate generation bounded and non-blocking for the open path. Cap per-strategy budget for the initial `g o` open, defer full annealing/VNS to explicit weight change or export, or otherwise ensure `open_optimizer` returns within a single frame (<100ms) while still producing valid candidates. Preserve determinism (D9) and banner/same-name constraints. `optimizer_set_weight` (`[`/`]`) must not re-run the full 6000-eval suite if the weight delta is small; regenerate is still live but bounded.
- **Graph visibility**: ensure `render_graph` always produces visible content. Fix camera fit to handle small/degenerate `area` without negative `pixel_size`, validate kitty scene has visible node rects before returning `true`, and otherwise fall through to box-drawing. No change to layout solver or cable index.

## Non-goals

- No new optimizer strategies or objective changes.
- No hardware integration.
- No change to controller geometry or panel layout.

