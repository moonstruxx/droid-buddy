## Purpose

Render hardware components grouped by physical controller type (P2B8, Faderbank, Notebuttons, Encoder, etc.) in labeled panels that mirror the physical hardware layout, providing clear visual organization of patch components.

## ADDED Requirements

### Requirement: Group components by controller type
The system SHALL group hardware components by their physical controller type (P2B8, Faderbank, Notebuttons, Encoder, Pot, Unusedfaders, etc.) based on the hardware token prefix and position.

#### Scenario: P2B8 components grouped
- **WHEN** a patch contains tokens B1.1-B1.8, L1.1-L1.8, P1.1-P1.2
- **THEN** all 18 components are rendered inside a single panel labeled "P2B8"

#### Scenario: Faderbank components grouped
- **WHEN** a patch contains fader-related tokens
- **THEN** those components are rendered inside a panel labeled "Faderbank"

### Requirement: Render controller panel with border and title
The system SHALL render each controller group as a bordered panel with a title showing the controller type name.

#### Scenario: Panel with title
- **WHEN** a controller panel is rendered
- **THEN** it displays a border with the controller type name as the title (e.g., " P2B8 ", " Faderbank ")

### Requirement: Position components in physical layout order
The system SHALL arrange components within each panel in the same order as they appear on the physical hardware (e.g., B1.1 through B1.8 left-to-right, top-to-bottom for P2B8).

#### Scenario: P2B8 button order
- **WHEN** P2B8 buttons are rendered
- **THEN** B1.1 appears first (left), B1.8 appears last (right), in physical order

### Requirement: Display component labels and state
The system SHALL display each component's label (e.g., "TRIG A", "CUTOFF", "CV IN 1") and its current state (ON/OFF for buttons, percentage for knobs, value for CV I/O) within its panel.

#### Scenario: Button state display
- **WHEN** a button component is rendered
- **THEN** it shows its label and current state (ON/OFF) with a visual indicator (● for on, ○ for off)

#### Scenario: Knob value display
- **WHEN** a knob component is rendered
- **THEN** it shows its label and current value as a percentage (e.g., "50%")

### Requirement: Handle overflow with scrolling or wrapping
The system SHALL handle panels that contain more components than fit in the available terminal width by wrapping to multiple rows or providing horizontal scrolling.

#### Scenario: Panel overflow
- **WHEN** a controller panel has more components than fit in one row
- **THEN** components wrap to additional rows within the panel

### Requirement: Support terminal resize
The system SHALL reflow the layout when the terminal is resized, preserving component visibility and panel structure.

#### Scenario: Terminal resized larger
- **WHEN** the terminal window is enlarged
- **THEN** panels expand to use available space, potentially showing more components per row

#### Scenario: Terminal resized smaller
- **WHEN** the terminal window is reduced
- **THEN** panels reflow to fit the smaller area, wrapping components as needed
