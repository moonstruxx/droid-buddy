## ADDED Requirements

### Requirement: Module-UI elements tint in the active shift colour

With a shift group active, module-UI elements belonging to the active group SHALL render in the group's theme token: the composed button-face outline and the glyph-cell colour follow the active group's token, reusing the same group→token match as the panel borders. Elements not in the active group SHALL render exactly as with no shift active. Unaddressed (dimmed, unlabelled) elements SHALL never receive shift colour. With no shift active every module-UI spec SHALL be unchanged.

#### Scenario: Member button tints on the module UI

- **WHEN** ShiftGroup::Group2 is active and a module-UI button belongs to Group2
- **THEN** its face outline (composed) or glyph colour renders in the theme's Group2 token

#### Scenario: Non-member element unchanged

- **WHEN** ShiftGroup::Group2 is active and a module-UI element belongs to Group1 (or no group)
- **THEN** the element renders exactly as with no shift active

#### Scenario: Unaddressed cells stay dim

- **WHEN** any shift group is active
- **THEN** unaddressed geometry cells still render dimmed and unlabelled with no shift tint and no hit rect

#### Scenario: No shift is a no-op

- **WHEN** no shift group is active
- **THEN** every module-UI cell renders exactly as before this change (byte-identical specs)
