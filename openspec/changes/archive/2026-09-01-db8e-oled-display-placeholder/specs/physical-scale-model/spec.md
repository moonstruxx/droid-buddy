# physical-scale-model Specification Delta

## MODIFIED Requirements

### Requirement: DB8E OLED display placeholder

The system SHALL render a bordered OLED display placeholder in the upper band of every DB8E faceplate on the physical 1:1 view, above the B-grid (above `y0_mm 38` inside the 6 HP module rect), with centered state text derived from the patch. The placeholder is part of the rack structure path so both skeleton and full presentations render it identically.

#### Scenario: Placeholder is visible in the DB8E upper band

- **WHEN** a patch contains a DB8E controller and the physical view renders
- **THEN** a single bordered rectangle appears in the upper band of that faceplate (above the B-grid), regardless of zoom or skeleton/full mode.

#### Scenario: Placeholder text is centered and derived from the patch

- **WHEN** the placeholder renders
- **THEN** its centered text is the derived DB8E display state: `"not used"` when the patch declares no DB8E, `"configuration error"` when the declared controller chain mismatches the wired chain (stub: otherwise `"connected"`), truncated/ellipsized to the rect width.

#### Scenario: Placeholder is rendered through the shared rack-structure path

- **WHEN** either `render_physical_skeleton` or `render_physical_full` draws the rack
- **THEN** the DB8E placeholder is drawn inside `render_rack_structure`'s per-module loop, so skeleton and full coincide (same rect, same text) per the D5 coincidence contract.

#### Scenario: Placeholder is not hit-testable

- **WHEN** the physical view publishes `component_rects` for handler hit-testing
- **THEN** the display placeholder does not publish a hit rect (it is decorative, not a component cell).
