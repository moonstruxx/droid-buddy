## Context

Single-crate Rust monolith. The pure modules (patch, graph, layout, physical, schema, validation, diff, geometry, optimize, latency, expression, events, favorites, plugin, and the pure part of config) have no terminal dependency and carry the app's behavior. The presentation and input layer (ui.rs, handler.rs, theme.rs, main.rs, kitty_protocol.rs, the graph_render.rs rasterizer) is ratatui/crossterm-bound. The `gui` feature already proves a winit/egui/egui-winit/wgpu window with a multiplexed event loop, a backend-neutral graph scene, and an egui painter. This pivot reuses that shell for the whole app. See proposal.md for the motivation.

## Goals / Non-Goals

**Goals:**
- One native process, one window, one `App` state. The pure modules stay untouched.
- Every surface renders in native pixels (anti-aliased, full color, real typography) and accepts native input.
- The rendering and rendering-test machinery shrinks, not grows. Delete the terminal stack and the `TestBackend` gallery.

**Non-Goals:**
- No `Spec` abstraction layer between surfaces and the painter. Each surface draws straight onto egui from the pure model, and tests assert on egui output directly.
- No dual-render path. The terminal is deleted, not kept alive.
- No redesign of the pure modules or the DROID domain model.

## Decisions

1. Reuse the existing winit/egui/wgpu shell instead of adopting eframe. The `gui` feature's `AppHandler` already owns the event loop and the graph canvas. eframe folds the same loop into one crate but drags a redundant wrapper and Cargo churn for no gain. (Alternative: eframe — rejected.)

2. Native-only, terminal deleted up front. No feature flag retains the terminal. A second renderer for six surfaces is dead weight the user does not want. (Alternative: keep the terminal behind a feature until teardown — rejected as the constraint overhead the user called out.)

3. gui.rs becomes a module, one file per surface. A single-file gui.rs would grow past a maintainable size and block parallel work. Per-surface files keep responsibilities clear and match the guardrail against catch-all files. The graph canvas moves to `src/gui/graph.rs`; physical, panels, viewer, picker, and overlays each get their own file under `src/gui/`.

4. Input rehosts, not redesigns. handler.rs keeps the key/mouse semantics but reads a neutral event shape from egui/winit instead of crossterm. Every binding keeps its meaning; only the event source changes. (Alternative: move input logic into gui.rs — rejected; it would scatter the binding rules already centralized in handler.rs.)

5. A native window has no character columns, so terminal-width concepts die. The sub-120-column collapse and the "renders degraded at N cols" hint are removed. Panes are pixel-sized; the physical view's mm-to-cell `ScreenMapping` becomes mm-to-pixel, and the ~2:1 cell aspect compensation goes away because pixels are square. Proportions get simpler.

6. Verification moves to egui output. Tests drive the pure model, then drive egui's headless `Context::run` to get a `FullOutput` and assert on shapes and labels. No GPU, no window, no `TestBackend`. The insta/ANSI/HTML gallery is deleted with the terminal.

7. Surface port order by value: physical (the user's primary representation) and panels first, then viewer/picker, then overlays. Faithful port of current behavior first; native polish (real anti-aliasing, hover states, spacing) is a consequence of the platform, not a separate redesign pass.

## Risks / Trade-offs

- [Risk] The locked egui/wgpu and winit versions are the only source of truth; the port does not bump them. → Mitigation: reuse the locked versions; consult current egui docs only if a surface needs an API the graph path does not already use.
- [Risk] The agent roster has no egui-native specialist, so closest-fit assignment may produce uneven egui code. → Mitigation: assign by layer (api-engineer for the window/integration/graph, layout-designer for visual surfaces, rusty for logic); create an `egui-engineer` via /make-engineer if surface work shows a persistent gap.
- [Risk] Native input (modifiers, focus, window events) has edge cases the terminal never hit. → Mitigation: target Linux, the app's stated platform; other platforms are out of scope for this change.
- [Risk] Deleting the terminal removes the only no-window test environment, and CI may run headless. → Mitigation: egui's headless `Context::run` gives deterministic output without a display; the perf-gate's window-free phases already run headless.

## Migration Plan

This is a rewrite of the presentation layer in one branch, with no partial rollout. Order: shell and dependency swap (produces a bare window), then surface ports (physical, panels, viewer, picker, overlays), then terminal teardown and the test/documentation sweep. Each surface port lands behind `cargo build` plus its shape/label test; the full verification gate runs at the end.

## Open Questions

- Landing surface: whether the physical 1:1 view or the controller panels is the default view on launch. This is a default-focus choice that does not change the specs or task breakdown; it can be settled during the panels/physical port.
