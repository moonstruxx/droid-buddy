# Controller Panels Specification

## Purpose

Render hardware components grouped by physical controller type (P2B8, Faderbank, Notebuttons, Encoder, etc.) in labeled panels that mirror the physical hardware layout, providing clear visual organization of patch components.

## Requirements

### Requirement: Group components by controller type
The system SHALL group hardware components by their physical controller type (P2B8, Faderbank, Notebuttons, Encoder, Pot, Unusedfaders, etc.) based on the hardware token prefix and position. Controller panels now contain modules rather than raw components directly.

#### Scenario: P2B8 components grouped
- **WHEN** a patch contains tokens B1.1-B1.8, L1.1-L1.8, P1.1-P1.2
- **THEN** all 18 components are rendered inside modules within a single panel labeled "P2B8"

#### Scenario: Faderbank components grouped
- **WHEN** a patch contains fader-related tokens
- **THEN** those components are rendered inside modules within a panel labeled "Faderbank"

### Requirement: Render controller panel with border and title
The system SHALL render each controller group as a bordered panel with a title showing the controller type name. Panels now contain module containers which in turn contain components.

#### Scenario: Panel with title
- **WHEN** a controller panel is rendered
- **THEN** it displays a border with the controller type name as the title (e.g., " P2B8 ", " Faderbank ")

#### Scenario: Panel contains modules
- **WHEN** a controller panel has components from multiple circuits
- **THEN** those components are first grouped into module containers, then the modules are arranged within the panel

### Requirement: Position components in physical layout order
The system SHALL arrange components within each module in the same order as they appear on the physical hardware (e.g., B1.1 through B1.8 left-to-right, top-to-bottom for P2B8). Modules are then arranged within panels based on their circuit order.

#### Scenario: P2B8 button order
- **WHEN** P2B8 buttons are rendered
- **THEN** B1.1 appears first (left), B1.8 appears last (right), in physical order within their module

### Requirement: Display component labels and state
The system SHALL display each component's label (e.g., "TRIG A", "CUTOFF", "CV IN 1") and its current state (ON/OFF for buttons, percentage for knobs, value for CV I/O) within its panel.

#### Scenario: Button state display
- **WHEN** a button component is rendered
- **THEN** it shows its label and current state (ON/OFF) with a visual indicator (● for on, ○ for off)

#### Scenario: Knob value display
- **WHEN** a knob component is rendered
- **THEN** it shows its label and current value as a percentage (e.g., "50%")

### Requirement: Handle overflow with scrolling or wrapping
The system SHALL handle panels that contain more modules than fit in the available terminal width by wrapping modules to multiple rows. Components within modules wrap based on module width.

#### Scenario: Panel overflow
- **WHEN** a controller panel has more components than fit in one row
- **THEN** components wrap to additional rows within the panel

#### Scenario: Panel overflow with modules
- **WHEN** a controller panel has more modules than fit in one row
- **THEN** modules wrap to additional rows, maintaining their internal component layout

### Requirement: Support terminal resize
The system SHALL reflow the layout when the terminal is resized, preserving component visibility and panel structure.

#### Scenario: Terminal resized larger
- **WHEN** the terminal window is enlarged
- **THEN** panels expand to use available space, potentially showing more components per row

#### Scenario: Terminal resized smaller
- **WHEN** the terminal window is reduced
- **THEN** panels reflow to fit the smaller area, wrapping components as needed

### Requirement: Module-aware layout calculation
The system SHALL calculate panel layout based on module dimensions rather than fixed component counts. Panel width accommodates the sum of module widths plus spacing.

#### Scenario: Variable-width module arrangement
- **WHEN** a panel contains modules of different widths (4HP, 8HP, 12HP)
- **THEN** they are arranged left-to-right with each taking space proportional to its width

#### Scenario: Panel wraps at terminal boundary
- **WHEN** the sum of module widths exceeds terminal width
- **THEN** subsequent modules wrap to the next row within the panel

### Requirement: Box LED-associated elements
An element with an associated LED (`led: Some(...)`) SHALL render as a single bordered cell: border color from the element's kind color (button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red); inside the border the element symbol, label, state, and the LED glyph reflecting the LED component's state. The LED component SHALL NOT render as its own standalone cell.

#### Scenario: P2B8 button with LED
- **WHEN** a P2B8 button section carries an `led = L1.N` association
- **THEN** B1.1 renders as a bordered box with white border showing the button's symbol/label/state and the LED's glyph/state inside

#### Scenario: Knob without LED
- **WHEN** a pot has no LED association
- **THEN** it renders as the two-line text cell (no border, no box)

### Requirement: Box geometry and hit-testing
Boxed cells use the updated component cell geometry (height 3). The published component geometry SHALL reflect the boxed cell for click hit-testing.

#### Scenario: Click on boxed cell
- **WHEN** the user clicks anywhere inside a bordered box
- **THEN** the element toggles/selects

#### Scenario: LED state changes
- **WHEN** the LED state of a boxed element changes
- **THEN** the glyph inside the box updates accordingly
