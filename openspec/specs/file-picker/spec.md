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