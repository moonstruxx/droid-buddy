## Purpose

Lets users switch between portrait and landscape orientation for the component panel layout in the DROID patch viewer.

## ADDED Requirements

### Requirement: Orientation switching

The system SHALL provide a mechanism to switch between portrait and landscape orientation for component panel layout.

#### Scenario: User switches to landscape orientation

- **WHEN** user presses the `o` key to toggle orientation
- **THEN** panel layout reflows from vertical (portrait) to horizontal (landscape) arrangement and the status bar displays "Orientation: Landscape"

#### Scenario: User switches back to portrait orientation

- **WHEN** user presses the `o` key again to toggle back
- **THEN** panel layout reflows from horizontal (landscape) to vertical (portrait) arrangement and the status bar displays "Orientation: Portrait"

### Requirement: Orientation state persists across patch loads

The system SHALL remember the last selected orientation and apply it when loading new patches.

#### Scenario: Orientation persists across patch switch

- **WHEN** user loads a different .ini patch while landscape orientation is active
- **THEN** the new patch renders in landscape orientation without requiring re-adjustment

### Requirement: Component reflow respects orientation

The system SHALL reflow components into appropriate panel arrangements when orientation changes, maintaining accessibility and readability.

#### Scenario: Component reflow in landscape mode

- **WHEN** orientation switches from portrait to landscape on a patch with 20+ components
- **THEN** components reorganize into horizontal rows within the available screen width, preserving labels and interaction areas

### Requirement: Default orientation is portrait

The system SHALL default to portrait orientation on startup unless a saved preference exists.

#### Scenario: Startup default orientation

- **WHEN** user launches the application
- **THEN** the application starts in portrait orientation with components arranged in vertical panels