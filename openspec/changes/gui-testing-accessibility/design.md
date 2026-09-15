## Context

The paint functions currently take `&Painter` which bypasses egui's widget system. The `Ui` type provides both `painter()` and `allocate_exact_size()` which creates AccessKit nodes. All three paint functions are called from `src/gui/mod.rs` inside a `context.run_ui` closure, so `Ui` is already available at the call site.

## Goals / Non-Goals

**Goals:**

- Make graph nodes, panel components, and physical cells queryable by label in egui_kittest
- Enable screen readers to discover UI elements through AccessKit
- Expand kittest test coverage to major user journeys

**Non-Goals:**

- Screen reader behavior testing (only making the tree available)
- Full AccessKit coverage of overlays, viewer, picker (follow-up change)
- Visual rendering changes

## Decisions

**D1: Ui parameter instead of Painter**

Change paint function signatures to accept `&mut Ui`. This is the minimal change that enables AccessKit node creation. Alternative: use `ui.scope()` for nesting, but that adds unnecessary complexity and could affect the layout.

**D2: WidgetInfo::labeled for all elements**

Use `WidgetInfo::labeled(WidgetType::Unknown, enabled, label)` as the annotation. The label carries the component/circuit identity. Alternative: per-kind WidgetType (Button, Slider) but the current render is presence-only, not interactive — the widget type doesn't affect behavior.

**D3: Incremental rollout**

Start with graph/panels/physical (the three main surfaces). Overlays, viewer, picker are a follow-up change. This keeps the change reviewable and limits the blast radius.

## Risks / Trade-offs

- **Signature change ripple**: Changing paint function signatures ripples through call sites in mod.rs. Mitigated by the small number of call sites (3 functions, ~5 call sites total).
- **Test updates**: Existing tests that call paint with a raw Painter will need updating to use `ctx.run_ui`. This is mechanical work.
- **Layout shift**: `allocate_exact_size()` reserves space, which could shift existing layouts. Mitigated by allocating the same size the painter was already using (no visual change intended).
