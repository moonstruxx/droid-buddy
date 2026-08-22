# Controller Panels Specification

## Purpose

Render hardware components grouped by physical controller type (P2B8, Faderbank, Notebuttons, Encoder, etc.) in labeled panels that mirror the physical hardware layout, providing clear visual organization of patch components.

## MODIFIED Requirements

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

### Requirement: Handle overflow with scrolling or wrapping
The system SHALL handle panels that contain more modules than fit in the available terminal width by wrapping modules to multiple rows. Components within modules wrap based on module width.

#### Scenario: Panel overflow
- **WHEN** a controller panel has more components than fit in one row
- **THEN** components wrap to additional rows within the panel

#### Scenario: Panel overflow with modules
- **WHEN** a controller panel has more modules than fit in one row
- **THEN** modules wrap to additional rows, maintaining their internal component layout

### Requirement: Module-aware layout calculation
The system SHALL calculate panel layout based on module dimensions rather than fixed component counts. Panel width accommodates the sum of module widths plus spacing.

#### Scenario: Variable-width module arrangement
- **WHEN** a panel contains modules of different widths (4HP, 8HP, 12HP)
- **THEN** they are arranged left-to-right with each taking space proportional to its width

#### Scenario: Panel wraps at terminal boundary
- **WHEN** the sum of module widths exceeds terminal width
- **THEN** subsequent modules wrap to the next row within the panel
