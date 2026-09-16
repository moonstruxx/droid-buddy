## Why

droid_tui's GUI paint routines (`paint_scene`, `paint_panels`, `paint_physical`) use raw `&Painter` calls that bypass egui's widget allocation system. This means:

- No AccessKit nodes are emitted, so screen readers cannot discover UI elements
- egui_kittest tests cannot query elements by label or role (only state-based verification works)
- The current 7 kittest tests only verify the rendering pipeline doesn't panic

## What Changes

- Restructure `paint_scene`, `paint_panels`, `paint_physical` from `fn(painter: &Painter, ...)` to `fn(ui: &mut Ui, ...)` so they can use `ui.allocate_exact_size()` for AccessKit node registration
- Add `response.widget_info()` with `WidgetInfo::labeled()` for each interactive element (graph nodes, panel components, physical cells)
- Expand kittest test suite from 7 to ~20 tests covering major user journeys

## Capabilities

### New Capabilities

- `gui-accessibility`: AccessKit annotations for screen reader support and egui_kittest query-by-label

### Modified Capabilities

- `signal-flow-graph`: graph paint gains AccessKit nodes
- `controller-panels`: panel paint gains AccessKit nodes
- `physical-scale-model`: physical paint gains AccessKit nodes

## Non-goals

- Screen reader testing (only making the tree available, not testing SR behavior)
- Full AccessKit coverage of every widget (start with graph, panels, physical)
- Visual rendering changes (paint output stays identical)

## Impact

- `src/gui/graph.rs`: paint_scene signature + WidgetInfo
- `src/gui/panels.rs`: paint_panels signature + WidgetInfo
- `src/gui/physical.rs`: paint_physical signature + WidgetInfo
- `src/gui/mod.rs`: dispatch calls updated
- Test infrastructure expansion
