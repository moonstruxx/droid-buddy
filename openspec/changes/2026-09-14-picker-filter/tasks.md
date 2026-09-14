## 1. App filter state

- [ ] 1.1 Add picker filter state to `App` (`picker_filter_active: bool`, `picker_filter: String`) with helpers `picker_filter_matches`, `filtered_picker_entries` or filtered view over `picker_entries`, and reset helpers (`clear_picker_filter`). Preserve `..` entry and favourite ordering when filtering. <!-- agent: rusty-engineer.build, touches: src/app.rs -->
- [ ] 1.2 Unit tests for filter helpers: empty filter shows all, lowercase matches both cases, uppercase matches only uppercase, `..` always visible, favourites filtered uniformly. <!-- agent: horst-engineer.build, touches: src/app.rs -->

## 2. Picker keybinding

- [ ] 2.1 Implement `Ctrl+f` latching toggle in `handle_picker_event`, character append while active (digits 0-9 passthrough to favourite fast-select, not appended), `Backspace` removes last char, `ESC`/`q`/selecting a patch resets filter; ensure Ctrl+f does not trigger favourite toggle. <!-- agent: rusty-engineer.build, touches: src/handler.rs -->
- [ ] 2.2 Add handler tests for each acceptance criterion: Ctrl+f toggle, filter string building, case rule, digit passthrough, ESC/q/Enter reset, Backspace. <!-- agent: horst-engineer.build, touches: src/handler.rs -->

## 3. Rendering

- [ ] 3.1 Update `picker_spec` to expose filter string and `paint_picker` to render `filter: <string>` line when active (below directory header), and render only filtered rows; keep `picker_index` clamped to filtered view. <!-- agent: rusty-engineer.build, touches: src/gui/picker.rs -->
- [ ] 3.2 Snapshot test for picker with active filter showing `filter: ab` and filtered rows. <!-- agent: horst-engineer.build, touches: src/gui/picker.rs, src/regression.rs -->

## 4. Verification

- [ ] 4.1 Run full verification gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`. <!-- agent: horst-engineer.build, touches: (none) -->
