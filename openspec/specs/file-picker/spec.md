# File Picker Specification

## Purpose

Provide an interactive file browser within the TUI for selecting and loading DROID `.ini` patch files from the filesystem.

## Requirements

### Requirement: Open file picker
The system SHALL display a file picker overlay when the user presses the `l` key and no patch is currently loaded, or when explicitly requested.

#### Scenario: Open file picker with no patch loaded
- **WHEN** the user presses `l` and no patch is loaded
- **THEN** a file picker overlay appears showing the current directory

#### Scenario: Open file picker with patch loaded
- **WHEN** the user presses `l` and a patch is already loaded
- **THEN** the file picker overlay appears, allowing the user to load a different patch

### Requirement: Navigate directories
The system SHALL allow the user to navigate up and down the directory tree using keyboard navigation (j/k or arrow keys) and select directories with Enter. When the picker is not at the filesystem root, the parent-directory entry SHALL render as the first entry labeled `..`; pressing Enter on it SHALL navigate to the parent directory without closing the picker. At the filesystem root, no `..` entry SHALL appear.

#### Scenario: Navigate into subdirectory
- **WHEN** the user highlights a directory and presses Enter
- **THEN** the file picker shows the contents of that directory

#### Scenario: Navigate to parent directory
- **WHEN** the user highlights the `..` entry and presses Enter
- **THEN** the file picker shows the parent directory and remains open

#### Scenario: No parent entry at root
- **WHEN** the file picker displays the filesystem root
- **THEN** no `..` entry is shown

### Requirement: Filter .ini files
The system SHALL highlight `.ini` files as selectable patch files and display non-`.ini` files as non-selectable (dimmed). Entries SHALL sort directories first, then `.ini` files.

#### Scenario: .ini file selectable
- **WHEN** the file picker displays a directory containing `.ini` files
- **THEN** `.ini` files are shown with normal brightness and can be selected

#### Scenario: Non-.ini file dimmed
- **WHEN** the file picker displays non-`.ini` files
- **THEN** those files are shown dimmed and cannot be selected

#### Scenario: Directories sort first
- **WHEN** a directory contains both subdirectories and `.ini` files
- **THEN** the subdirectories (including `..` when not at root) sort before the `.ini` files

### Requirement: Load selected patch
The system SHALL load the selected `.ini` file as a patch when the user presses Enter on it, closing the file picker and rendering the patch view.

#### Scenario: Load patch from file picker
- **WHEN** the user highlights an `.ini` file and presses Enter
- **THEN** the file is parsed, the patch is loaded, the file picker closes, and the patch view is rendered

### Requirement: Cancel file picker
The system SHALL allow the user to cancel the file picker and return to the previous view by pressing `Esc`.

#### Scenario: Cancel file picker
- **WHEN** the user presses `Esc` while the file picker is open
- **THEN** the file picker closes and the previous view is restored

### Requirement: Favourites section in picker
The system SHALL render favourited `.ini` files in a pinned section at the top of the file picker list, above the directory listing. The favourites section and directory listing are both navigable using the same keys.

#### Scenario: Favourites above directory listing
- **WHEN** the file picker is open and favourites exist
- **THEN** the favourites section appears at the top, followed by the directory listing below

#### Scenario: Navigation crosses section boundary
- **WHEN** the user navigates past the last favourite entry
- **THEN** the cursor moves into the directory listing

#### Scenario: Navigation wraps within section
- **WHEN** the user is in the favourites section and navigates up past the first entry
- **THEN** the cursor stays on the first favourite entry

#### Scenario: Toggle favourite with f key
- **WHEN** the user highlights any file entry (favourite or directory-listing) and presses `f`
- **THEN** the entry's favourite status is toggled and the picker re-renders accordingly

### Requirement: Filter picker entries

The system SHALL support an inline filter in the file picker that narrows the visible entry list by substring matching.

#### Scenario: Toggle filter with Ctrl+f

- **WHEN** the user presses `Ctrl+f` while the picker is open
- **THEN** filter latching toggles on (or off if already active) and the picker shows `filter: <string>` (empty string initially)

#### Scenario: Filter string building

- **WHEN** filter latching is active and the user types characters
- **THEN** each non-digit character is appended to the filter string and the visible entries are filtered to those whose file name contains the filter string per the case rule; the filter line updates to `filter: <string>`

#### Scenario: Case rule

- **WHEN** the filter string contains a lowercase character
- **THEN** that character matches both lowercase and uppercase in file names; an uppercase filter character matches only uppercase

#### Scenario: Digit passthrough while filtering

- **WHEN** filter latching is active and the user presses a digit `0`–`9`
- **THEN** the digit does NOT append to the filter string; instead it fast-selects the favourite slot as in the non-filter case (Ctrl/Alt-held digits do nothing)

#### Scenario: Backspace in filter

- **WHEN** filter latching is active and the user presses `Backspace`
- **THEN** the last character of the filter string is removed (no effect if empty) and the visible list re-filters

#### Scenario: Reset filter

- **WHEN** the user presses `Esc`, `q`, or successfully selects a patch while filter latching is active
- **THEN** filter latching is cleared and the filter string is reset to empty before closing/navigating

#### Scenario: Parent entry always visible

- **WHEN** the filter is active
- **THEN** the `..` parent-directory entry remains visible regardless of the filter string
