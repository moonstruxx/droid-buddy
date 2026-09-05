# Design: keybinding-help-modal

## Context

The handler routes keys through a fixed priority chain (edit overlay > picker > validation > optimizer > prefix > graph > viewer > panels), and the renderer mirrors it with a top z-layer for the edit overlay, validation modal, and optimizer menu. The help modal slots into both chains as another top-level surface. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- One `?` key that works from every view and shows that view's bindings
- Close paths that match the app's existing modal conventions (Esc) plus the explicit `q` and click-outside
- Content that stays correct as views gain keys, without duplicating the binding list

**Non-Goals:**
- No keybinding reconfiguration or user customization
- No search or filter inside the modal
- No change to existing key behavior outside the modal's lifetime

## Decisions

### D1 — Help modal is a top-level surface in the priority chain
The `showing_help` branch sits directly below the edit overlay in `handle_event` (overlay > help > picker > validation > optimizer > prefix > graph > viewer > panels). While open it eats all keys except `Esc` and `q`, both of which close it and return `false`. `q` closing help instead of quitting is the one deliberate override of the global quit key, scoped to the modal's lifetime.
- Alternative considered: treating help as a passive overlay that lets keys fall through. Rejected: the modal must be dismissible from any state, and falling through would let `q` quit underneath it.

### D2 — `?` opens from any view, matched without a modifier guard
`KeyCode::Char('?')` is handled right after the edit-overlay branch, before the picker branch, so it works from every surface including the picker. The key arrives with the SHIFT modifier (Shift+/), so the match carries no modifier guard, the same convention as `+` (Shift+=). The edit overlay still eats it, since that surface is text input.
- Alternative considered: handling `?` only in the normal-key branch. Rejected: the requirement is view-dependent help, and the picker/validation/optimizer surfaces would be unreachable.

### D3 — View-dependent content lives in a pure module
`src/help.rs` (no terminal dependency, matching `diff.rs`/`graph.rs`/`validation.rs`) owns `HelpView` (Panels, Viewer, Graph, Quad, Validation, Optimizer, Picker), `active_view(&App)` mirroring the handler priority chain, and `keybindings(HelpView) -> Vec<(&'static str, &'static str)>`. The renderer and the tests share one source of truth for what each view's keys are; adding a key means editing one table.
- Alternative considered: building the list inline in `ui.rs`. Rejected: it would couple rendering to the binding data and make the content untestable without a terminal.

### D4 — Renderer publishes the modal rect
`render_help_modal` publishes `app.help_modal_rect` each frame (the `component_rects`/`graph_node_rects` pattern), and the handler hit-tests click-outside against it. The rect is cleared per frame. The click-outside check runs before the quad/graph mouse branches so a click over any surface closes the modal instead of dragging a node or toggling a component.
- Alternative considered: computing the rect in the handler from a fixed formula. Rejected: the renderer owns layout (ADR 4); the handler must not guess where the modal landed.

### D5 — Theme tokens follow the optimizer precedent
`help_modal_border` and `help_modal_selected_bg` are added to all three palettes in `src/theme.rs`, mirroring `optimizer_modal_border`/`optimizer_selected_bg`. No hardcoded colors.

## Risks / Trade-offs

- [`q` override could surprise] → Mitigation: it is scoped strictly to the modal's open state; the modal is transient and the status/header can note the close keys.
- [Binding list drifts from the handler] → Mitigation: `keybindings` is a single table per view in `help.rs`; the handler tests assert the `?` open/close paths, and the snapshot matrix pins the rendered rows.
- [Modal over the picker adds a render path] → Mitigation: the picker early-return in `render` gains a help check before returning, keeping the modal on top without restructuring the dispatch.