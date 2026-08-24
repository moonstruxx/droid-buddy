# Shift Visualization Specification

## Purpose

Provide clear visual feedback when shift modifier keys (1-4) are held, highlighting affected controller panels with a colored border frame and dimming unrelated panels so users can immediately see which components a shift key modifies.

## Requirements

### Requirement: Shift key activation
The system SHALL activate a shift group when the user presses the corresponding number key (1, 2, 3, or 4).

#### Scenario: Press shift key 1
- **WHEN** the user presses `1`
- **THEN** ShiftGroup::Group1 is activated and the status bar shows "SHIFT 1 ACTIVE"

#### Scenario: Press shift key 2
- **WHEN** the user presses `2`
- **THEN** ShiftGroup::Group2 is activated and the status bar shows "SHIFT 2 ACTIVE"

### Requirement: Colored border frame for active shift group
The system SHALL render a colored border frame around controller panels that contain components belonging to the active shift group. Each shift group's color comes from the active theme's shift-group tokens, which are guaranteed pairwise distinct. The default `classic` theme preserves today's mapping: Group1=Yellow, Group2=Cyan, Group3=Magenta, Group4=Green.

#### Scenario: Classic mapping preserved
- **WHEN** the classic theme is active and shift key 1 is pressed with matching panels present
- **THEN** those panels display a bold yellow border with the shift key label in the title

#### Scenario: Theme-dependent colors
- **WHEN** a non-classic theme is active and a shift group is activated
- **THEN** affected panels use that theme's shift-group token for the group, distinct from every other group's token

#### Scenario: Active shift panel highlighted
- **WHEN** ShiftGroup::Group1 is active and a panel contains Group1 components
- **THEN** that panel displays a bold border in the theme's Group1 color with the shift key label in the title (yellow under `classic`)

#### Scenario: Inactive shift panel dimmed
- **WHEN** ShiftGroup::Group1 is active and a panel contains Group2 components
- **THEN** that panel displays a dim gray border

### Requirement: Clear shift with Esc
The system SHALL clear the active shift group when the user presses `Esc`, returning all panels to their default (non-highlighted) border style.

#### Scenario: Clear shift
- **WHEN** the user presses `Esc` while a shift group is active
- **THEN** the active shift is cleared, all panels return to default borders, and the status bar shows "Shift cleared"

### Requirement: Status bar shows active shift
The system SHALL display the currently active shift group in the status bar with the group's theme color and bold styling.

#### Scenario: Status bar with active shift
- **WHEN** ShiftGroup::Group3 is active and the classic theme is active
- **THEN** the status bar shows "SHIFT 3 ACTIVE" in magenta bold text

#### Scenario: Themed status text
- **WHEN** any theme is active and a shift key is held
- **THEN** the status bar shows "SHIFT N ACTIVE" in that theme's token for the group, bold

#### Scenario: Status bar with no active shift
- **WHEN** no shift group is active
- **THEN** the status bar shows only the general status message without shift indicator

### Requirement: Shift visualization works with mouse
The system SHALL maintain shift visualization state correctly when the user interacts via mouse (clicking components while a shift is active).

#### Scenario: Click component during shift
- **WHEN** a shift group is active and the user clicks a component in the highlighted panel
- **THEN** the component state toggles and the shift visualization remains active
