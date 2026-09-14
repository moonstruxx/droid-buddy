# Change: picker filter for file picker

## Why

The file picker lists all entries in the current directory and favourites pinned section. With many patches, favourites and ini files mixed, scrolling to find a patch by name is slow. A quick inline filter that narrows the visible list without leaving the picker speeds up selection and matches the existing vim-style navigation.

## What Changes

- Ctrl+f toggles a latching filter mode while the picker is open. While active, typed characters append to a filter string displayed as `filter: <string>`. Digits 0-9 continue to fast-select favourite slots. Backspace edits the filter string. ESC, q, or selecting a patch resets the filter. Lowercase filter characters match case-insensitively, uppercase matches only uppercase.
- `App` gains picker filter state (active flag + string) and a helper to compute filtered visible entries while preserving favourite-section ordering and `..` handling.
- `handle_picker_event` gains Ctrl+f toggle, filter-string editing, and filtered navigation; `refresh_picker_entries` is filtered through the active string.
- `picker_spec` / `paint_picker` display the filter line when active.
- No new dependencies, no new config file.

## Capabilities

### New Capabilities

- `file-picker/filter`: inline substring filter in the file picker with Ctrl+f latching, case rule, digit passthrough, and reset semantics.

### Modified Capabilities

- `file-picker`: picker now supports an optional filter that narrows visible entries; existing navigation, favourites, and selection requirements still apply to the filtered view.

## Impact

- Affected code: `src/app.rs`, `src/handler.rs`, `src/gui/picker.rs`, `src/regression.rs`.
- No breaking changes to patch loading.

## Non-goals

- No regex or fuzzy matching — plain substring with the specified case rule only.
- No persistent filter across picker opens.
- No filtering of the favourites pinned section separately from the directory listing (both are filtered uniformly except `..` always stays).
