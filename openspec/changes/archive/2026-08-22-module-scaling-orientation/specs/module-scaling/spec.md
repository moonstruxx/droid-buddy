## Purpose

Lets users scale component sizes in the DROID patch viewer to accommodate different display configurations and hardware revisions.

## ADDED Requirements

### Requirement: Component scaling presets

The system SHALL provide component scaling presets of 50%, 100%, 150%, and 200% for all component types.

#### Scenario: User selects 150% scaling

- **WHEN** user presses the `+` key to increase scaling
- **THEN** component sizes increase to 150% of their original size and the status bar displays "Scaling: 150%"

#### Scenario: User selects 50% scaling

- **WHEN** user presses the `-` key to decrease scaling below 100%
- **THEN** component sizes decrease to 50% of their original size and the status bar displays "Scaling: 50%"

### Requirement: Scale factor persistence across patch loads

The system SHALL remember the last selected scale factor and apply it when loading new patches.

#### Scenario: Scale factor persists across patch switch

- **WHEN** user loads a different .ini patch while 150% scaling is active
- **THEN** the new patch renders at 150% scaling without requiring re-adjustment

### Requirement: Scale factor is independent of orientation

The system SHALL maintain separate scale factor values for portrait and landscape orientations.

#### Scenario: Scale switches with orientation

- **WHEN** user switches from portrait to landscape orientation
- **THEN** the scale factor switches to the landscape preset while component positions reflow accordingly

### Requirement: Minimum component size is preserved

The system SHALL ensure that no component scales below 40 pixels width or 20 pixels height, regardless of the selected preset.

#### Scenario: Minimum size enforcement

- **WHEN** user selects 50% scaling on a patch with small components
- **THEN** component dimensions remain above the minimum thresholds and do not collapse