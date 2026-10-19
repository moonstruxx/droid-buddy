## ADDED Requirements

### Requirement: Drive roots expose the other available drives on Windows

When the file picker is at a drive root on Windows, it SHALL list every other existing drive as a directory entry so navigation is not confined to the drive the app was started on. Pressing Enter on such an entry SHALL switch the picker to that drive's root. The entries SHALL follow the same directory-first ordering as ordinary subdirectories, and the inline filter SHALL NOT hide them. Filesystem roots on Linux and macOS SHALL behave exactly as before: no parent entry, no drive entries.

#### Scenario: Drive root lists other drives

- **WHEN** the picker shows `C:\` and drives `D:` and `E:` also exist
- **THEN** the listing contains `D:\` and `E:\` entries, each rendered as a directory

#### Scenario: Enter on a drive entry switches drive

- **WHEN** the user highlights `D:\` and presses Enter
- **THEN** the picker shows the contents of `D:\` and the highlight returns to the first entry

#### Scenario: Non-Windows root unchanged

- **WHEN** the picker shows `/` on Linux or macOS
- **THEN** no drive entries appear and no `..` entry is shown
