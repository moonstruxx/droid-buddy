## MODIFIED Requirements

### Requirement: Plugin file discovery

The system SHALL load circuit definitions from `*.toml` files in the `droid-tui/plugins/` directory under the platform data directory (defined by the `configuration` capability: `$XDG_CONFIG_HOME` or `~/.config` on Linux and macOS, `%APPDATA%` on Windows), at startup, before rendering begins. A missing plugins directory is not an error.

#### Scenario: Default plugin directory

- **WHEN** the app starts and the platform `droid-tui/plugins/` directory does not exist
- **THEN** the app starts normally with only the embedded schema, and no warning is printed

#### Scenario: Plugin files present

- **WHEN** the app starts and `plugins/` contains `*.toml` files
- **THEN** every parseable file contributes its circuit definitions to the active schema before the first frame renders

#### Scenario: Windows plugin directory

- **WHEN** the app runs on Windows with `%APPDATA%` set
- **THEN** plugin files are discovered under `%APPDATA%\droid-tui\plugins\`
