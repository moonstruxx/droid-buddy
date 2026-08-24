## MODIFIED Requirements

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

### Requirement: Status bar shift indicator
The system SHALL display the currently active shift group in the status bar with the group's theme color and bold styling.

#### Scenario: Classic status text
- **WHEN** the classic theme is active and shift key 3 is held
- **THEN** the status bar shows "SHIFT 3 ACTIVE" in magenta bold text

#### Scenario: Themed status text
- **WHEN** any theme is active and a shift key is held
- **THEN** the status bar shows "SHIFT N ACTIVE" in that theme's token for the group, bold
