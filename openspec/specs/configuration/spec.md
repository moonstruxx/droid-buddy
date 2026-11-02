# configuration Specification

## Purpose
Persistent user preferences for the TUI, stored in a single TOML file so settings survive across sessions.

## Requirements

### Requirement: Config file discovery

The system SHALL read user preferences from `config.toml` inside the `droid-tui` directory under the platform data directory. On Linux and macOS the platform data directory is the XDG config home (`$XDG_CONFIG_HOME` when set and absolute, otherwise `$HOME/.config`). On Windows it is `%APPDATA%`. The file is loaded once at startup, before the UI initializes. The same resolver SHALL serve the label store, the favourites store, and the plugin directory, so all four live beside each other on every platform.

#### Scenario: Fresh machine

- **WHEN** no droid-tui directory and no config file exist
- **THEN** the app starts normally with built-in defaults and prints nothing about configuration

#### Scenario: Standard location honored

- **WHEN** `$XDG_CONFIG_HOME` is set to `/tmp/cfg`
- **THEN** the app reads `/tmp/cfg/droid-tui/config.toml`

#### Scenario: Windows uses the roaming app data directory

- **WHEN** the app runs on Windows and `%APPDATA%` is `C:\Users\anna\AppData\Roaming`
- **THEN** the app reads and writes `C:\Users\anna\AppData\Roaming\droid-tui\config.toml`

#### Scenario: Unresolvable data directory

- **WHEN** neither the platform variable nor the home directory is set
- **THEN** the app starts with built-in defaults, and a save reports that the data directory cannot be determined instead of panicking

#### Scenario: Non-absolute XDG value

- **WHEN** `$XDG_CONFIG_HOME` is a relative path
- **THEN** it is treated as unset and the resolver falls back to the home directory rule

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

### Requirement: Labels table in user configuration
The system SHALL load and save `[labels]` (`layers_enabled`, `max_shift_layer`) in XDG `config.toml` alongside `theme`, with warn-once on malformed values and clamping of `max_shift_layer` to 1..8.

#### Scenario: Missing table defaults
- **WHEN** `config.toml` has no `[labels]` table
- **THEN** `layers_enabled = true` and `max_shift_layer = 4` are used and no edit to other keys occurs on save

### Requirement: Optional plugins configuration
The system SHALL accept an optional `[plugins]` section in `config.toml` with `dir` (a plugin directory override, defaulting to `$XDG_CONFIG_HOME/droid-tui/plugins/`) and `enabled` (a boolean, default `true`). When `enabled` is `false`, no plugin files are loaded. The section's absence SHALL behave as defaults; malformed values SHALL warn once on stderr and fall back to defaults, matching the existing config handling.

#### Scenario: Default plugins behavior
- **WHEN** `config.toml` has no `[plugins]` section
- **THEN** plugins load from the default directory and are enabled

#### Scenario: Plugins disabled
- **WHEN** `config.toml` sets `[plugins] enabled = false`
- **THEN** no plugin files are loaded and the embedded schema is used alone

#### Scenario: Custom plugin directory
- **WHEN** `config.toml` sets `[plugins] dir = "/some/other/dir"`
- **THEN** plugin files are discovered in that directory instead of the default

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

### Requirement: Rack module inventory

The system SHALL load an optional `modules` list on each `[[physical.rack.rows]]` entry of `config.toml`, naming the modules that row physically holds in left-to-right order. Each entry SHALL be a module key `"{controller} {instance}"` with a 1-based instance as the patch numbers it (`P2B8 1`, `E4 2`), or a bare controller name for a single-instance module (`B32`, `G8`, `X7`, `R2M`, `R2C`). The entries `master` and `CV I/O` SHALL both name the master slot and SHALL resolve to the master model resolved for the loaded patch, so one inventory describes either master. An entry naming a controller with no known geometry SHALL emit one warning on stderr and be skipped without affecting the remaining entries. An absent or empty `modules` list SHALL leave the previous behaviour: the chain is derived from the patch and rows are auto-packed.

A module listed in a row SHALL be placed in that row; `assign` SHALL NOT move it. `assign` SHALL keep applying to modules the inventory does not list.

#### Scenario: Inventory places modules in their rows

- **WHEN** `config.toml` declares two rows whose `modules` lists name four and three modules
- **THEN** the physical view shows those modules in list order, the first four in the first row and the next three in the second

#### Scenario: Absent inventory keeps today's rendering

- **WHEN** `config.toml` has no `modules` list on any row
- **THEN** the chain is derived from the loaded patch exactly as before and rows are auto-packed

#### Scenario: Unknown module entry warns and is skipped

- **WHEN** a row lists `"P3B7 1"`, a controller with no geometry entry
- **THEN** one warning naming the entry is printed and the row renders its other modules in order

#### Scenario: The master alias follows the patch

- **WHEN** a row lists `master` and the loaded patch declares `master=18`
- **THEN** that row slot renders the MASTER18 faceplate, not the MASTER face

#### Scenario: Instance-qualified keys bind patch components

- **WHEN** a row lists `P2B8 1` and `P2B8 2` and the patch addresses `B1.x` and `B2.x`
- **THEN** the `B1.x` components render on the first faceplate and the `B2.x` components on the second

#### Scenario: A listed module ignores assign

- **WHEN** `assign` maps a module to row 1 while a row's `modules` list names it in row 0
- **THEN** the module renders in row 0

#### Scenario: An unlisted module still honours assign

- **WHEN** a patch module is absent from every row's `modules` list and `assign` places it in row 1
- **THEN** the module renders in row 1
