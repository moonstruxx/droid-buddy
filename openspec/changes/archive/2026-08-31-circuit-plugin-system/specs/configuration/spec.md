# Configuration — delta

## MODIFIED Requirements

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