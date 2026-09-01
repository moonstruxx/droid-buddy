# Physical Fader Rendering Specification

## Purpose

Render F-family faders (P8S8 20 mm sliders, M4 60 mm motorized faders) on the physical 1:1 view as vertical tracks with a position-driven amber LED bar, replacing the flat boxed percentage cell, and guarantee that adjoined element cells never publish overlapping `component_rects` at any supported zoom preset.

## Requirements

### Requirement: Vertical fader track rendering

The system SHALL render F-family fader elements as a vertical track glyph whose fill is proportional to the component's current value, on the physical view's fader cell. Track brightness/position replaces the flat `◉ %` boxed face for these elements.

#### Scenario: Fader renders a vertical track proportional to value

- **WHEN** a patch declares a fader (F-family, P register) on an M4 or P8S8 controller at a given value
- **THEN** the physical view renders that fader cell with a vertical track whose filled portion corresponds to the value (0% at the bottom of the track, 100% at the top), scaled to the cell height.

#### Scenario: Zero value empties the track

- **WHEN** the fader value is 0%
- **THEN** the rendered track is unfilled (no fill), while the cell still shows a zero state.

#### Scenario: Full value fills the track

- **WHEN** the fader value is 100%
- **THEN** the rendered track is fully filled from bottom to top.

### Requirement: Amber LED bar on faders

The system SHALL render an amber LED bar alongside the fader track whose brightness reflects the fader position, mirroring the physical slider cap LED — brighter with higher value, dimmer with lower value — using a dedicated amber LED-bar theme token.

#### Scenario: LED bar tracks fader position

- **WHEN** a fader is at a given value
- **THEN** the rendered LED-bar fill mirrors the fader's position (empty at 0%, full at 100%), rendered in the amber LED-bar token.

#### Scenario: LED bar uses the amber token

- **WHEN** the fader LED bar renders
- **THEN** all its filled cells use the dedicated amber token (distinct from the generic LED red), in every theme palette.

### Requirement: Fader-cell hit rect matches the rendered cell

The system SHALL publish the fader cell's `component_rects` rect equal to the rendered vertical-track cell, so hit-testing (hover/click/scroll-to-adjust) resolves against the drawn cell, not a virtual surrounding box.

#### Scenario: Fader hit rect equals rendered cell

- **WHEN** the physical view renders a fader at any zoom preset
- **THEN** the published component rect for that fader equals the drawn track cell.

### Requirement: Adjoined element cells never publish overlapping rects

The system SHALL clamp adjoined element-cell spans at draw time so that, at any supported zoom preset, two distinct element cells never publish overlapping `component_rects`; a shared rounding column is resolved deterministically (first cell wins the column, the neighbor is clamped out) rather than left to iteration order.

#### Scenario: Adjoined cells do not overlap at non-default zoom

- **WHEN** two adjacent element cells round to share a screen column at a non-default zoom preset (e.g. 150%)
- **THEN** their published rects do not overlap: one cell owns the shared column and the neighbor is clamped so no column belongs to both, and hover/click in any column resolves deterministically.

#### Scenario: Overlap-free at every zoom preset

- **WHEN** a patch is rendered at zoom 75%, 100%, 150%, or 200%
- **THEN** the strict no-overlap assertion over all published `component_rects` holds at each preset (not only at 100%).