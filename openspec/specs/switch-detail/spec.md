# switch-detail Specification

## Purpose
Give Switch components (S tokens) a distinct visual identity and visible position/state detail so they read differently from buttons and can represent positional switches.

## Requirements

### Requirement: Dedicated switch theme token

The system SHALL provide a dedicated `switch` semantic color token in the theme layer across all three built-in palettes (`classic`, `terminal`, `mono`), and Switch cells SHALL render using that token. In `classic` the token SHALL keep the current white value so existing snapshots remain byte-identical.

#### Scenario: Switch uses its own token

- **WHEN** a Switch component renders under any theme
- **THEN** its glyph and state text use the theme's `switch` token, independent of the `button` token.

#### Scenario: Classic palette unchanged

- **WHEN** the `classic` palette renders a Switch cell
- **THEN** the color equals the previous button color (white), producing no snapshot diff.

### Requirement: Switch value rendering

The system SHALL render a Switch whose state is `ComponentState::Value(v)` as `◉ {:.0}%` (filled glyph plus percentage), mirroring knob/encoder value display, while retaining the `▣ ON` / `□ OFF` rendering for the `On`/`Off` states.

#### Scenario: Positional switch shows percentage

- **WHEN** a Switch is in `ComponentState::Value(0.35)`
- **THEN** the cell shows `◉ 35%`.

#### Scenario: Binary switch unchanged

- **WHEN** a Switch is in `ComponentState::On` or `ComponentState::Off`
- **THEN** the cell shows `▣ ON` or `□ OFF` exactly as before.
