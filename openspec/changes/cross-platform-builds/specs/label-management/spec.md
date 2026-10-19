## MODIFIED Requirements

### Requirement: Per-patch isolation and atomic persistence

The system SHALL key store buckets by canonicalized absolute patch path and SHALL persist `labels.toml` in the platform data directory (`$XDG_CONFIG_HOME/droid-tui/labels.toml` or `~/.config/droid-tui/labels.toml` on Linux and macOS, `%APPDATA%\droid-tui\labels.toml` on Windows) via atomic tmp to rename, with warn-once on corrupt toml (fallback empty).

#### Scenario: Two masters isolated

- **WHEN** `droid_mpfs5melody2.ini` and `droid_mpfs5drum.ini` both have `B1.1`
- **THEN** a label set on one patch does not appear on the other

#### Scenario: Save failure names the reason

- **WHEN** no platform data directory can be resolved
- **THEN** the save reports that the config directory cannot be determined and the in-memory edit is unchanged
