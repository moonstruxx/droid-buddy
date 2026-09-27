# Tasks

## 1. Sync help tables with the handler

- [x] 1.1 Sync the `Panels` keybinding table in `src/help.rs` with the handler's live bindings: add `\` (toggle left-pane vertical split), `g s` (open select-state menu), `Tab`/`Shift+Tab` (cycle pane focus) and verify every existing row still matches a real handler binding; verify with `cargo test --lib help` <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/help.rs] -->
- [x] 1.2 Sync the `Graph` keybinding table: add `h` (toggle column/force layout), `f` (dependency filter), `i` (influence filter), `g s` (select-state menu) and correct the cable-tension rows from `[/]` to `Alt+[`/`Alt+]`; verify with `cargo test --lib help` <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/help.rs] -->
- [x] 1.3 Fill the `Physical` keybinding table with the handler's real keys: `+`/`-` (zoom presets), arrow keys (pan rack), `j`/`k` (navigate), keeping `s` (skeleton toggle) and `Esc`; verify with `cargo test --lib help` <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/help.rs] -->
- [x] 1.4 Audit the `Viewer`, `Validation`, `Optimizer`, and `Picker` tables against the handler and correct any drift found (the optimizer's `0`/`1` snap-weight rows and the viewer's `[/]` split rows must match the handler's modifier-free binds); verify with `cargo test --lib help` <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/help.rs] -->

## 2. Parity tests

- [x] 2.1 Extend the `help.rs` table tests so each view's rows assert the keys added/corrected in tasks 1.1-1.4 (the existing `keybindings_reflect_graph_center_fit_and_g_c_chord` and `keybindings_include_carousel_key` tests set the pattern); verify with `cargo test --lib help` <!-- agent: horst-engineer.build, depends_on: [1.4], touches: [src/help.rs] -->
- [x] 2.2 Add a handler-side parity test in `src/handler.rs` pinning that `?` opens the modal and the keys added in 1.1-1.4 dispatch through `handle_event` without being eaten by an earlier branch (mirror `question_mark_opens_help_from_every_view`); verify with `cargo test --lib handler` <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/handler.rs] -->

## 3. Verification gate

- [x] 3.1 Run the full verification gate and confirm green: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` <!-- agent: rusty-engineer.fast, depends_on: [2.2], touches: [] -->