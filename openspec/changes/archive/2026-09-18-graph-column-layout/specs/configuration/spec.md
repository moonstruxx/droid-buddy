## ADDED Requirements

### Requirement: Layout table in user configuration

The system SHALL load and save a `[layout]` table in XDG `config.toml` alongside `theme`, `[labels]`, and `[latency]`, with warn-once on malformed values and fallback to defaults. The table SHALL carry `mode` (default `column`) and `ordering` (default `strict`). `mode` SHALL be one of `column` or `force`; `ordering` SHALL be one of `strict` or `barycenter`. Unknown values SHALL warn once on stderr and fall back to the default.

#### Scenario: Missing table defaults

- **WHEN** `config.toml` has no `[layout]` table
- **THEN** `mode = column` and `ordering = strict` are used and no edit to other keys occurs on save

#### Scenario: Force mode configured

- **WHEN** `config.toml` sets `[layout] mode = "force"`
- **THEN** the graph opens with the force-directed arrangement as the session default

#### Scenario: Barycenter ordering configured

- **WHEN** `config.toml` sets `[layout] ordering = "barycenter"`
- **THEN** the column arrangement orders nodes within a column by crossing-minimization sweeps

#### Scenario: Malformed layout value falls back

- **WHEN** `config.toml` sets `[layout] mode = "sideways"`
- **THEN** the app warns once on stderr and uses `mode = column`