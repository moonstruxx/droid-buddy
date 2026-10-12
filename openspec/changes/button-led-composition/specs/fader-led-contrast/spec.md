## MODIFIED Requirements

### Requirement: State indicators stay bright while paused

The fader lit strip and the LED indication MUST render at full brightness while processing is paused; all other cell chrome (glyphs, labels, unlit track, a composed button face's outline) keeps the existing pause-dim.

#### Scenario: Strip and dot exempt

- **WHEN** processing is paused and a fader/LED cell is visible
- **THEN** the lit strip and LED dot render undimmed while the cell's text and unlit track render dimmed.

#### Scenario: Folded fill exempt

- **WHEN** processing is paused and a composed button face with a folded LED is visible
- **THEN** the LED's inner fill renders undimmed while the face outline and label render dimmed.

### Requirement: LED ring with brightness floor

Every standalone LED cell MUST render as an outline ring plus a filled core, and a folded LED (the `module-ui` capability's "Button face and LED composition") MUST render as the inner fill of its element's face. Both MUST apply the brightness floor (about one quarter) to the rendered extent and intensity, so an off LED still shows its field. Patch-supplied RGB colors keep their hue.

#### Scenario: Off LED shows its field

- **WHEN** an LED is at value 0
- **THEN** the cell shows a ringed field with a dim core instead of black-on-black.

#### Scenario: Folded LED shows its field

- **WHEN** a folded LED belonging to a button face or touch plate is at value 0
- **THEN** the face shows a small dim core inside its outline rather than no fill at all.
