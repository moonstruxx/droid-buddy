## ADDED Requirements

### Requirement: Status bar composes each segment exactly once
The status bar SHALL compose each segment (Scale, Orientation, and any other status segments) exactly once per frame; no segment SHALL be duplicated regardless of the input method (keyboard or mouse) that triggered the state change.

#### Scenario: Scale and orientation shown once
- **WHEN** a patch is loaded and the scale/orientation state is changed
- **THEN** the status bar shows each of the Scale and Orientation segments exactly once

#### Scenario: No duplicate segments after mouse interaction
- **WHEN** the user changes state via mouse input (e.g., scrolling a knob)
- **THEN** the status bar still composes each segment exactly once