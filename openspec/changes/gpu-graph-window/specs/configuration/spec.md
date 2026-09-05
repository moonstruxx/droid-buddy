# configuration

## ADDED Requirements

### Requirement: GUI window configuration

The config file SHALL accept an optional `[gui]` section with a single boolean key `graph_window`, defaulting to `false`. When `true` and the `gui` feature is enabled, `g g` opens the GPU graph window instead of the terminal tile. Malformed values SHALL warn once on stderr and fall back to the default, matching the existing config handling.

#### Scenario: Default behavior

- **WHEN** `config.toml` has no `[gui]` section
- **THEN** `graph_window` is `false` and `g g` opens the terminal tile

#### Scenario: Window preferred

- **WHEN** `config.toml` sets `[gui] graph_window = true` and the `gui` feature is enabled
- **THEN** `g g` opens the GPU graph window

#### Scenario: Malformed value

- **WHEN** `config.toml` sets `[gui] graph_window = "yes"`
- **THEN** the app warns once on stderr and falls back to `false`