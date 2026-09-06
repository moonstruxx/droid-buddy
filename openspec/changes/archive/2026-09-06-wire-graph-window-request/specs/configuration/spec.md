# configuration

## ADDED Requirements

### Requirement: GUI window flag seeded at startup

When the `gui` feature is enabled, the app SHALL seed the runtime graph-window preference from the `[gui] graph_window` config value during startup (`seed_app`), so the windowed run-loop's per-frame request consumption acts on the configured preference.

#### Scenario: Window preferred is honored

- **WHEN** the app starts with `[gui] graph_window = true` and the `gui` feature is enabled
- **THEN** the runtime graph-window preference is `true` and `g g` requests the GPU graph window

#### Scenario: Default remains terminal tile

- **WHEN** the app starts with `[gui] graph_window` absent (default `false`)
- **THEN** the runtime graph-window preference is `false` and `g g` opens the terminal tile