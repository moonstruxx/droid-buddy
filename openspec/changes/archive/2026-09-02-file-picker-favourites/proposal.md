# Change: file-picker-favourites

## Why

The file picker lists every `.ini` file in a flat sorted list. Users who repeatedly load the same few patches (their main performance patch, a reference patch, a test fixture) must navigate to the same directory and scroll to the same entry every time. Adding a favourites list pins frequently-used patches to the top of the picker, saving navigation time.

## What Changes

- New `FavoritesStore` persisting a list of absolute paths to favourited files in `~/.config/droid-tui/favourites.toml`, following the same XDG + atomic-write pattern as `LabelStore`.
- `App` gains a `favorites` field, loaded at startup and saved on every toggle.
- `refresh_picker_entries` shows favourited `.ini` files in a pinned section at the top of the picker list, marked with a star glyph.
- A new keybinding (`f`) toggles the currently highlighted entry as a favourite while the picker is open.
- Unit tests for persistence, integration, and the toggle keybinding; visual validation of the favourites section in the picker UI.

## Capabilities

### New Capabilities

- `file-picker/favourites`: persistent favourites list in the file picker, pinned entries at the top of the list with a star glyph, toggle keybinding.

### Modified Capabilities

- `file-picker`: the picker now renders a favourites section above the directory listing; the existing navigation and selection requirements still apply to both sections.

## Impact

- Affected code: `src/app.rs`, `src/handler.rs`, `src/ui.rs`, new `src/favorites.rs` (or inline in `src/config.rs`), `src/regression.rs`, `fixtures/`.
- No new dependencies.
- No breaking changes.

## Non-goals

- No remote or cloud-synced favourites.
- No tagging, grouping, or categorisation of favourites.
- No search or filtering within the picker.
- No recent-files history (distinct from favourites).
