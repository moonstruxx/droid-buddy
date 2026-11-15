# Spec Delta

## ADDED Requirements

### Requirement: Graph grouping table in user configuration

The system SHALL load and save a `[graph]` table in `config.toml` alongside `theme`, `[labels]`, `[latency]`, `[layout]`, `[physical]` and `[plugins]`, with warn-once on malformed values and fallback to defaults. The table SHALL carry `group_enabled` (default `true`) and `min_group_size` (default `5`, clamped to 2..=64). An absent table SHALL leave grouping enabled at the default minimum size and SHALL NOT be rewritten on save unless a `[graph]` setting changed.

#### Scenario: Missing table defaults

- **WHEN** `config.toml` has no `[graph]` table
- **THEN** grouping is enabled with `min_group_size = 5`

#### Scenario: Malformed value falls back

- **WHEN** `config.toml` sets `[graph] min_group_size = "many"`
- **THEN** the app warns once on stderr and uses the default of 5

#### Scenario: Out-of-range minimum is clamped

- **WHEN** `config.toml` sets `[graph] min_group_size = 1`
- **THEN** the effective minimum is clamped to 2

#### Scenario: Grouping disabled

- **WHEN** `config.toml` sets `[graph] group_enabled = false`
- **THEN** the graph opens with every node drawn individually
