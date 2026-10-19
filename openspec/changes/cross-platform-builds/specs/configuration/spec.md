## MODIFIED Requirements

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
