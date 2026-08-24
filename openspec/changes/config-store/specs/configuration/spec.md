## Purpose

Persistent user preferences for the TUI, stored in a single TOML file so settings survive across sessions.

## ADDED Requirements

### Requirement: Config file discovery
The system SHALL read user preferences from `config.toml` inside the `droid-tui` directory under the XDG config home (`$XDG_CONFIG_HOME` when set, otherwise `~/.config`). The file is loaded once at startup, before the terminal UI initializes.

#### Scenario: Fresh machine
- **WHEN** no droid-tui directory and no config file exist
- **THEN** the app starts normally with built-in defaults and prints nothing about configuration

#### Scenario: Standard location honored
- **WHEN** `$XDG_CONFIG_HOME` is set to `/tmp/cfg`
- **THEN** the app reads `/tmp/cfg/droid-tui/config.toml`

### Requirement: Theme selection key
The config file SHALL support a single global string key `theme`. Its value SHALL be resolved against the built-in theme catalog using canonical name matching (case-insensitive; `-`/`_`/space equivalent).

#### Scenario: Known theme selected
- **WHEN** the config contains `theme = "TokyoNight"` (any theme name valid in principle)
- **THEN** the matching built-in theme is active for the session

#### Scenario: Unknown theme name
- **WHEN** the config names a theme that does not exist in the catalog
- **THEN** the app warns once on stderr naming the bad value and the valid choices, and continues with the default theme

### Requirement: Malformed config handling
A config file that fails to parse SHALL NOT crash or block startup.

#### Scenario: Broken TOML
- **WHEN** the config file contains invalid TOML syntax
- **THEN** the app warns once on stderr and starts with built-in defaults

#### Scenario: Unknown keys ignored
- **WHEN** the config contains keys the current version does not know
- **THEN** they are ignored without warning and all known keys still apply

### Requirement: Atomic write path
The config API SHALL support writing the current settings back to the config file atomically (write to a temporary file in the same directory, then rename over the target), creating the config directory on demand.

#### Scenario: Write creates directory and file
- **WHEN** settings are written while no config directory exists yet
- **THEN** the directory and `config.toml` are created and contain the full current settings
