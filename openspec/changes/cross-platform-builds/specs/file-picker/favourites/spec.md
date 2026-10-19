## MODIFIED Requirements

### Requirement: Favourites store persistence

The system SHALL persist a list of favourited file paths in `favourites.toml` inside the platform data directory (`~/.config/droid-tui/favourites.toml` or `$XDG_CONFIG_HOME/droid-tui/favourites.toml` on Linux and macOS, `%APPDATA%\droid-tui\favourites.toml` on Windows), using the same resolver and atomic write pattern as the label store.

#### Scenario: Save favourites on toggle

- **WHEN** the user toggles a file as a favourite
- **THEN** the updated favourites list is written atomically to the favourites store file

#### Scenario: Load favourites at startup

- **WHEN** the application starts
- **THEN** the favourites list is loaded from the store file (or an empty list if the file is missing or malformed)
