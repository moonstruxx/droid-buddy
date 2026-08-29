## MODIFIED Requirements

### Requirement: Display component labels and state
The system SHALL display each component's label (e.g., "TRIG A", "CUTOFF", "CV IN 1") and its current state (ON/OFF for buttons, percentage for knobs, value for CV I/O) within its panel. Labels longer than the available cell width SHALL truncate with a trailing ellipsis (…) while preserving cell geometry and hit-testing.

#### Scenario: Button state display
- **WHEN** a button component is rendered
- **THEN** it shows its label and current state (ON/OFF) with a visual indicator (● for on, ○ for off)

#### Scenario: Knob value display
- **WHEN** a knob component is rendered
- **THEN** it shows its label and current value as a percentage (e.g., "50%")

#### Scenario: Over-long label ellipsized
- **WHEN** a component's label exceeds the available cell width
- **THEN** the label truncates with a trailing ellipsis (…) and the cell geometry and hit rect are unchanged

### Requirement: Module-aware layout calculation
The system SHALL calculate panel layout based on module dimensions rather than fixed component counts. Panel width accommodates the sum of module widths plus spacing. Rows within a panel SHALL maintain uniform vertical spacing regardless of whether the cells they contain render as boxed (height 3) or unboxed two-line cells.

#### Scenario: Variable-width module arrangement
- **WHEN** a panel contains modules of different widths (4HP, 8HP, 12HP)
- **THEN** they are arranged left-to-right with each taking space proportional to its width

#### Scenario: Panel wraps at terminal boundary
- **WHEN** the sum of module widths exceeds terminal width
- **THEN** subsequent modules wrap to the next row within the panel

#### Scenario: Uniform vertical row spacing
- **WHEN** a panel mixes boxed and unboxed component cells in adjacent rows
- **THEN** rows maintain consistent vertical rhythm without irregular gaps

### Requirement: Box LED-associated elements
An element with an associated LED (`led: Some(...)`) SHALL render as a single bordered cell: border color from the active theme's token for the element's kind (button, knob, cv-in, cv-out, led; in the default `classic` theme these are button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red); inside the border the element symbol, label, state, and the LED glyph reflecting the LED component's state. The LED component SHALL NOT render as its own standalone cell.

An element's LED association is detected from two sources in its `.ini` section: a bare `led = L.N` entry (single element per section), or a numbered circuit LED param `ledN = L.M` (e.g. `led11 = L1.1`) that shares its numeric suffix with a same-suffix element entry (`button11 = B1.1`, `pot11 = P1.1`, `encoder11 = E1.1`, `switch11 = S1.1`, `fader11 = M1.1`, ...) in the same section — the DROID convention for circuits like `matrixmixer` that address elements and LEDs by a shared matrix-position suffix. The `ledN` value (`L.M`) is authoritative for the LED hardware token; its serial-position-dependent numbering is encoded by the patch author, not derived by the parser.

The boxed path SHALL support every control kind that can carry a resolvable LED association — button, knob/pot, encoder, switch, fader — rendering each kind's state inside the box (ON/OFF with a glyph for buttons and switches, percentage for knobs/encoders/faders). When the available cell width is narrower than the box content, the cell SHALL either shrink its content to fit inside a complete box or fall back to unboxed two-line rendering; it SHALL NEVER emit partial border fragments.

#### Scenario: P2B8 button with LED
- **WHEN** a P2B8 button section carries an `led = L1.N` association and the classic theme is active
- **THEN** B1.1 renders as a bordered box with white border showing the button's symbol/label/state and the LED's glyph/state inside

#### Scenario: Numbered circuit LED param pairs button and LED
- **WHEN** a circuit section carries `button11 = B1.1` and `led11 = L1.1` (shared suffix `11`) and any theme is active
- **THEN** B1.1 renders as a single bordered box with L1.1's glyph folded inside, and L1.1 does not render as a standalone cell

#### Scenario: Encoder with LED
- **WHEN** a circuit section carries `encoder11 = E1.1` and `led11 = L1.2` (shared suffix `11`) and any theme is active
- **THEN** E1.1 renders as a single bordered box showing its percentage and the LED glyph, and L1.2 does not render as a standalone cell

#### Scenario: Switch with LED
- **WHEN** a circuit section carries `switch11 = S1.1` and `led11 = L1.3` (shared suffix `11`) and any theme is active
- **THEN** S1.1 renders as a single bordered box showing its switch glyph and the LED glyph, and L1.3 does not render as a standalone cell

#### Scenario: Fader with LED
- **WHEN** a circuit section carries `fader11 = M1.1` and `led11 = L1.4` (shared suffix `11`) and any theme is active
- **THEN** M1.1 renders as a single bordered box showing its value and the LED glyph, and L1.4 does not render as a standalone cell

#### Scenario: Knob without LED
- **WHEN** a pot has no LED association and any theme is active
- **THEN** it renders as the two-line text cell (no border, no box)

#### Scenario: Kind colors follow the theme
- **WHEN** any theme is active and an element with an LED association renders
- **THEN** its box border uses that theme's token for the element's kind

#### Scenario: Narrow width does not garble the box
- **WHEN** the available cell width is smaller than the box content for an LED-associated element
- **THEN** the cell renders with complete borders — content shrinks to fit or the cell falls back to unboxed two-line rendering, never partial border fragments