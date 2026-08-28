## ADDED Requirements

### Requirement: Labels table in user configuration
The system SHALL load and save `[labels]` (`layers_enabled`, `max_shift_layer`) in XDG `config.toml` alongside `theme`, with warn-once on malformed values and clamping of `max_shift_layer` to 1..8.

#### Scenario: Missing table defaults
- **WHEN** `config.toml` has no `[labels]` table
- **THEN** `layers_enabled = true` and `max_shift_layer = 4` are used and no edit to other keys occurs on save

