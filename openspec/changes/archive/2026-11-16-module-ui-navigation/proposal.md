# Proposal

## Why

The labels fit deliberately overflows the pane — it is the greater of the whole-rack fit and the zoom every written label needs — so on the 17-faceplate `fixtures/physical_multirow_rack.ini` it lands at **1.44** against a **0.370** rack fit: the pane shows about a quarter of the rack, and the rest is reachable only by blind arrow-key panning or a middle-drag. There is no overview, no direct jump to a place, and no way back to automatic fitting once a pan has latched the view. The signal-flow graph solved exactly this (`paint_minimap`, `minimap_world_point`, click-to-pan, drag-to-region-zoom, `Shift+C` refit); the module UI has none of it, and `Char('C')` on that pane is unbound while the graph answers it.

## What Changes

- **A minimap in the module UI pane.** A pure `physical::rack_minimap` model over the same rack extent `physical_overflow` already computes, a 160 pt panel whose height follows the rack's aspect (clamped 56–120 pt), the case outline, one rect per module, the focused module highlighted, and the viewport box clamped into the map when the rack fits.
- **Gestures.** A click centres the pane on that rack point; a drag of three points or more fits the zoom to the dragged region; `Esc` cancels the marquee; the wheel over the map stays unbound. The same rules as the graph's, including "drawn even when the scene fits".
- **The panel claims its pointer area first.** Published as `physical_minimap_panel_egui` and checked before element hit-testing, so a click on the map never focuses an element and a wheel over it never adjusts a value.
- **`c` / `Shift+C` on the module UI pane.** `c` centres the focused element, falling back to centring the rack when nothing is focused. `Shift+C` runs the labels fit **and clears `physical_nav_latched`**, so the four automatic triggers (view open, patch load, rack change, pane resize) fit again — the re-arm no key offers today. This is the pair the graph already has (`handle_graph_camera_key`).
- **Documentation**: the pane's help table, `docs/views.md`, `docs/keys.md`, and a blessed `docs/assets/module-ui.png` showing the map.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `module-ui`: **ADDED** "Module UI minimap" — the map's presence and content, its pointer precedence, click-to-centre, drag-to-region-zoom, `Esc`, the always-drawn rule, and its absence in the performance view.
- `keybinding`: **ADDED** "Module UI camera keys" — `c` centres with its rack fallback, `Shift+C` fits and re-arms automatic fitting, the help table lists both, and the minimap gestures are documented.

## Impact

- `src/physical.rs` — the pure `rack_minimap` model and its transform inverse.
- `src/app.rs` — the published `physical_minimap_panel_egui` field and the centre-on helpers (`physical_center_on_component`, `physical_center_on_rack`) as pure offset mutations over `physical_pane_rect`.
- `src/gui/physical.rs` — the minimap payload on `PhysicalSpec`, its paint routine, and publishing the panel rect from `paint_physical`.
- `src/handler.rs` — panel precedence in the pointer path, press/drag/release, `Esc`, `c` and `Shift+C`.
- `src/help.rs` — the module-UI key table rows.
- `docs/views.md`, `docs/keys.md`; regenerated `DESIGN.md` / `ARCHITECTURE.md`; blessed `docs/assets/module-ui.png`.
- `src/regression.rs` — click mapping, region zoom, pointer precedence, and the two keys end to end.
- No new theme token: the map draws in the existing `accent`, `muted` and `focus_border` tokens.

## Non-goals

- No change to the signal-flow graph, its minimap, or `minimap_world_point`.
- No module titles in the map: at ~9 pt per module on a 17-module rack the boundaries read and text never does.
- No tinting of a module rect by its dominant element kind, and no per-element marks in the map.
- No map in the performance view, which is already the compact view.
- No change to the two fits, the zoom presets, the wheel semantics, the pane title's segments, or the pane layout.
- No persistence of map state, and no new configuration key.
