# file-picker/favourites Specification

## Purpose
Let users pin frequently-used patch files to the top of the file picker for quick access, persisting the list across sessions.

## Requirements

### Requirement: Favourites store persistence
The system SHALL persist a list of favourited file paths in `~/.config/droid-tui/favourites.toml`, using the same XDG base directory and atomic write pattern as the existing label store.

#### Scenario: Save favourites on toggle
- **WHEN** the user toggles a file as a favourite
- **THEN** the updated favourites list is written atomically to the favourites store file

#### Scenario: Load favourites at startup
- **WHEN** the application starts
- **THEN** the favourites list is loaded from the store file (or an empty list if the file is missing or malformed)

### Requirement: Toggle favourite status
The system SHALL allow the user to toggle the currently highlighted entry as a favourite by pressing `f` while the file picker is open.

#### Scenario: Mark entry as favourite
- **WHEN** the user highlights a file entry and presses `f` and the entry is not currently a favourite
- **THEN** the entry is added to the favourites list and the picker re-renders with the entry marked

#### Scenario: Unmark entry as favourite
- **WHEN** the user highlights a file entry and presses `f` and the entry is currently a favourite
- **THEN** the entry is removed from the favourites list and the picker re-renders with the entry unmarked

#### Scenario: Toggle on directory entry
- **WHEN** the user highlights a directory entry (including `..`) and presses `f`
- **THEN** no change occurs (directories cannot be favourited)

### Requirement: Favourites display in picker
The system SHALL render favourited `.ini` files in a pinned section at the top of the file picker list, visually distinct from the directory listing below.

#### Scenario: Favourites section shown
- **WHEN** the file picker is open and one or more favourites exist
- **THEN** a favourites section appears at the top of the picker, separated from the directory listing

#### Scenario: Star glyph for favourites
- **WHEN** a favourited file is rendered in the picker
- **THEN** the entry displays a star glyph (e.g. `★`) before the filename

#### Scenario: No favourites section when empty
- **WHEN** the file picker is open and no favourites exist
- **THEN** no favourites section is rendered; only the directory listing is shown

### Requirement: Favourite entries are selectable
The system SHALL treat favourited entries as selectable `.ini` files, supporting the same navigation and loading behaviour as directory-listing entries.

#### Scenario: Navigate to favourite
- **WHEN** the user navigates into the favourites section
- **THEN** the cursor moves through favourited entries using the same keys (j/k, arrows)

#### Scenario: Load from favourites
- **WHEN** the user presses Enter on a favourited entry
- **THEN** the file is loaded as a patch and the picker closes

#### Scenario: Enter on favourite from any directory
- **WHEN** a favourited file's absolute path points to a file outside the current directory
- **THEN** the file loads successfully regardless of the picker's current directory
