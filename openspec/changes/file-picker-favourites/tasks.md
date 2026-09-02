## 1. FavoritesStore persistence

- [x] 1.1 Create `src/favorites.rs` with `FavoritesStore` type (serde `Vec<String>` of absolute paths), `load`/`save`/`save_to_dir` methods following the `LabelStore` XDG + atomic-write pattern (`favourites.toml` in `~/.config/droid-tui/`), and `toggle`/`is_favourite` helpers. <!-- agent: rusty-engineer.build, touches: src/favorites.rs -->
- [x] 1.2 Add unit tests for `FavoritesStore`: load missing file yields empty list, save round-trips, toggle adds/removes paths, malformed TOML yields empty list with stderr warning. <!-- agent: horst-engineer.build, touches: src/favorites.rs -->

## 2. App integration

- [x] 2.1 Add `favorites: FavoritesStore` field to `App`, load it in `App::new` (or `load_patch` init path), and wire `favorites.save()` into the toggle path. Wire `mod favorites;` in `src/lib.rs`. <!-- agent: rusty-engineer.build, touches: src/app.rs, src/lib.rs -->
- [x] 2.2 Add `picker_entries_with_favourites` method (or modify `refresh_picker_entries`) that prepends favourited `.ini` files (matching absolute paths) to the picker list, marked with a `★` prefix. Ensure navigation index offsets correctly. <!-- agent: rusty-engineer.build, touches: src/app.rs -->

## 3. Keybinding

- [x] 3.1 Add `f` key handler in `handle_picker_event` that toggles the highlighted entry's favourite status via `FavoritesStore::toggle`, persists the store, and refreshes the picker entries. <!-- agent: rusty-engineer.build, touches: src/handler.rs -->
- [x] 3.2 Add test for `f` key: open picker, highlight an `.ini` entry, press `f`, verify entry is marked favourite; press `f` again, verify unmarked. <!-- agent: horst-engineer.build, touches: src/handler.rs -->

## 4. UI rendering

- [x] 4.1 Modify `render_picker` in `src/ui.rs` to render the favourites section (if non-empty) above the directory listing, with `★` prefix on each favourited entry and a visual separator between sections. <!-- agent: layout-designer-engineer.build, touches: src/ui.rs -->
- [x] 4.2 Add visual snapshot test for the picker with favourites: render picker at 80x24 with a mock favourites list, capture `insta` snapshot. <!-- agent: horst-engineer.build, touches: src/regression.rs, src/snapshots/ -->

## 5. Integration tests

- [x] 5.1 Add integration test: create temp dir with `.ini` files, favourite one via `FavoritesStore`, open picker, verify favourite appears at top with star glyph, press Enter, verify patch loads. <!-- agent: horst-engineer.build, touches: src/handler.rs, src/ui.rs -->
- [ ] 5.2 Run full verification gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`. <!-- agent: horst-engineer.build, touches: (none) -->
