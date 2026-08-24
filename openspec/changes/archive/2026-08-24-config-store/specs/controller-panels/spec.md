## MODIFIED Requirements

### Requirement: Box LED-associated elements
An element with an associated LED (`led: Some(...)`) SHALL render as a single bordered cell: border color from the active theme's token for the element's kind (button, knob, cv-in, cv-out, led; in the default `classic` theme these are button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red); inside the border the element symbol, label, state, and the LED glyph reflecting the LED component's state. The LED component SHALL NOT render as its own standalone cell.

#### Scenario: P2B8 button with LED
- **WHEN** a P2B8 button section carries an `led = L1.N` association and the classic theme is active
- **THEN** B1.1 renders as a bordered box with white border showing the button's symbol/label/state and the LED's glyph/state inside

#### Scenario: Knob without LED
- **WHEN** a pot has no LED association and any theme is active
- **THEN** it renders as the two-line text cell (no border, no box)

#### Scenario: Kind colors follow the theme
- **WHEN** any theme is active and an element with an LED association renders
- **THEN** its box border uses that theme's token for the element's kind
