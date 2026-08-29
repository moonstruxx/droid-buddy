## MODIFIED Requirements

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