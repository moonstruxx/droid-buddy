# Delta: file-picker — picker filter

## ADDED Requirements

### Requirement: Filter picker entries

The system SHALL support an inline filter in the file picker that narrows the visible entry list by substring matching.

#### Scenario: Toggle filter with Ctrl+f

- **WHEN** the user presses `Ctrl+f` while the picker is open
- **THEN** filter latching toggles on (or off if already active) and the picker shows `filter: <string>` (empty string initially)

#### Scenario: Filter string building

- **WHEN** filter latching is active and the user types characters
- **THEN** each non-digit character is appended to the filter string and the visible entries are filtered to those whose file name contains the filter string per the case rule; the filter line updates to `filter: <string>`

#### Scenario: Case rule

- **WHEN** the filter string contains a lowercase character
- **THEN** that character matches both lowercase and uppercase in file names; an uppercase filter character matches only uppercase

#### Scenario: Digit passthrough while filtering

- **WHEN** filter latching is active and the user presses a digit `0`–`9`
- **THEN** the digit does NOT append to the filter string; instead it fast-selects the favourite slot as in the non-filter case (Ctrl/Alt-held digits do nothing)

#### Scenario: Backspace in filter

- **WHEN** filter latching is active and the user presses `Backspace`
- **THEN** the last character of the filter string is removed (no effect if empty) and the visible list re-filters

#### Scenario: Reset filter

- **WHEN** the user presses `Esc`, `q`, or successfully selects a patch while filter latching is active
- **THEN** filter latching is cleared and the filter string is reset to empty before closing/navigating

#### Scenario: Parent entry always visible

- **WHEN** the filter is active
- **THEN** the `..` parent-directory entry remains visible regardless of the filter string

