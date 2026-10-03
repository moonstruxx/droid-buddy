# Spec Delta

## MODIFIED Requirements

### Requirement: Fail load on Error, modal error list with navigation

Patches with at least one `Severity::Error` SHALL fail to load (`app.patch=None`) and open a modal error list. Patches with only `Warning`/`Hint` SHALL load and SHALL NOT open the modal: the status bar SHALL report the finding count with a hint that `e` opens the list, so a warnings-only patch keeps its normal key handling (including app chords such as `g g`). In every case the modal lists all issues sorted by `(line,col)` and is reachable on demand with `e`.

#### Scenario: Error blocks patch display

- **WHEN** validation produces ≥1 Error
- **THEN** panels remain empty, `showing_validation=true`, status `Load failed: N errors — press 'e' to view`, and the modal lists all issues sorted by `(line,col)` with `L{line}:{col} [E/W/H] [code] message`

#### Scenario: Warnings load without opening the modal

- **WHEN** validation produces only `Warning`/`Hint` findings
- **THEN** the patch loads, `showing_validation=false`, the status bar reports the finding count with a hint that `e` opens the list, and app chords (`g g`, `l`, `?`) keep working until the user presses `e`

#### Scenario: Modal navigation jumps to source

- **WHEN** the modal is open and user presses `Enter` on a selected issue
- **THEN** the source viewer scrolls to `issue.span.line` and the issue's span is highlighted; `j/k` moves `validation_cursor`, `Esc`/`e` dismisses the modal
