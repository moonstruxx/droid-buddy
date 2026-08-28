## ADDED Requirements

### Requirement: Influenced cell background wash

The system SHALL render every hardware cell (boxed LED-cells and two-line text cells inside modules) whose hardware token is in the active modifier's structural influence set with a background wash in that modifier's stable hue while the modifier is active (momentary hold or latched). Cells not in the influence set SHALL be dimmed while a modifier is active. When no modifier is active the panel renders exactly as before. Geometry, hit-testing, and module grouping are unchanged.

#### Scenario: Hold tints influenced cells

- **WHEN** `B1.1` is held and its influence includes `P1.1` and `B1.3` but not `B1.2`
- **THEN** `P1.1` and `B1.3` cells show the `B1.1` hue background, `B1.2` is dimmed, all other panels unchanged.

#### Scenario: Additive latches blend

- **WHEN** `B1.1` (cyan) and `B1.2` (magenta) are both latched
- **THEN** cells influenced by either show their respective hue (union); a cell influenced by both shows the most-recently latched hue.

#### Scenario: No modifier active

- **WHEN** no modifier is held or latched
- **THEN** all cells render with the existing kind/shift styling and no wash.

#### Scenario: Narrow terminal

- **WHEN** the terminal is narrow and a modifier is active
- **THEN** the wash remains per-cell and does not alter layout or wrapping.
