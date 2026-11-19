# Spec Delta

## MODIFIED Requirements

### Requirement: Labels table in user configuration
The system SHALL load and save `[labels]` (`layers_enabled`, `max_shift_layer`, `performance_scale`) in XDG `config.toml` alongside `theme`, with warn-once on malformed values, clamping of `max_shift_layer` to 1..8, and clamping of `performance_scale` to 0.5..=4.0. An absent `performance_scale` SHALL mean 1.0, and an absent `[labels]` table SHALL NOT be rewritten on save unless a `[labels]` setting changed.

#### Scenario: Missing table defaults
- **WHEN** `config.toml` has no `[labels]` table
- **THEN** `layers_enabled = true`, `max_shift_layer = 4` and `performance_scale = 1.0` are used and no edit to other keys occurs on save

#### Scenario: Scale round-trips
- **WHEN** the performance label scale is set to 1.15 and the config is saved
- **THEN** a later start reads back 1.15 and the performance view opens at 115 %

#### Scenario: Scale is clamped
- **WHEN** `config.toml` sets `performance_scale = 12.0`
- **THEN** the value is clamped to 4.0 and the app starts normally

#### Scenario: Malformed scale warns once
- **WHEN** `config.toml` sets `performance_scale = "big"`
- **THEN** the app warns once on stderr and uses 1.0
