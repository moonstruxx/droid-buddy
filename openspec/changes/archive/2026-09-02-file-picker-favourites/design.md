## Context

The file picker (`src/app.rs` / `src/ui.rs`) currently renders a flat sorted list of directory entries and `.ini` files. Users who repeatedly load the same patches must navigate to the same directory and scroll to the same entry every time. The existing `LabelStore` (per-patch labels in `~/.config/droid-tui/labels.toml`) demonstrates the XDG + atomic-write pattern already in use.

## Goals / Non-Goals

**Goals:**
- Persistent favourites list stored in a TOML file under the XDG config directory
- Favourited entries pinned at the top of the picker with a visual marker
- Toggle keybinding (`f`) while the picker is open
- Navigation and loading behaviour identical to directory-listing entries

**Non-Goals:**
- Remote/cloud-synced favourites
- Tagging, grouping, or categorisation
- Search or filtering within the picker
- Recent-files history (distinct from favourites)

## Decisions

### D1: Standalone `FavoritesStore` type (not merged into `LabelStore`)

`LabelStore` is keyed by canonicalized patch path and stores per-patch label overrides. Favourites are a global list of absolute paths, unrelated to per-patch data. Keeping them separate avoids coupling the two stores and keeps each file's schema simple.

**Alternative considered:** Adding a `favourites: Vec<String>` field to `LabelStore`. Rejected because `LabelStore` is per-patch keyed and a global list would break that contract.

### D2: Separate file `favourites.toml` (not in `config.toml`)

`config.toml` holds theme and plugin settings. Favourites are user data that changes on every toggle; `config.toml` is rarely written. A separate file avoids unnecessary writes to the config file and keeps the two concerns isolated.

### D3: File paths stored as absolute strings

Favourites must resolve regardless of the picker's current directory. Storing absolute paths (canonicalized when the file exists, absolute-joined when it doesn't) matches `LabelStore::canonical_key` behaviour and avoids relative-path ambiguity.

### D4: Two-section picker layout (favourites + directory listing)

The picker renders a favourites section at the top (when non-empty), followed by the existing directory listing. Navigation crosses the boundary naturally: the cursor moves from the last favourite into the first directory entry. This avoids a mode-switch or separate overlay.

### D5: Toggle on `f` key, no mouse support initially

The picker is keyboard-driven today (j/k, Enter, Esc). Adding `f` as the toggle key fits the existing pattern. Mouse support can be added later if needed but is not required for the initial implementation.

### D6: Star glyph (`★`) for favourited entries

A single character marker is consistent with the picker's text-based rendering. Using `★` (U+2605) is visible and distinct from the `▶` selection marker.

## Risks / Trade-offs

- [Stale paths] → Favourites reference absolute paths that may be deleted or moved. The picker already shows missing files as non-selectable; no special handling needed.
- [File I/O on every toggle] → Atomic tmp→rename is fast for small files. The store is tiny (tens of entries at most). Acceptable.
- [Navigation complexity] → Two-section layout adds cursor-index arithmetic. Keep the favourites count as a known offset; the existing index-based navigation extends cleanly.

## Migration Plan

No migration needed. A missing `favourites.toml` yields an empty list. Existing functionality is unchanged.
