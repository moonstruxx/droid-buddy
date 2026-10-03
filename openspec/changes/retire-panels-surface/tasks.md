# Tasks

## 1. Retire the Panels surface

- [x] 1.1 Drop `PanelsFrame` from the `use crate::gui::{...}` import in `src/handler.rs` and delete `handle_panels_frame` together with the tests that build `PanelsFrame`; run `cargo test --lib handler::tests` and `cargo test --lib regression` green <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [x] 1.2 Delete `src/gui/panels.rs` and remove its wiring from `src/gui/mod.rs` (`mod panels;` and the `pub(crate) use panels::PanelsFrame;` re-export); fix the `panels::paint_panels` reference in the `src/gui/physical.rs` doc comment. Verify with `cargo check --all-targets` and `cargo clippy --all-targets --all-features --locked -- -D warnings` (no unused/dead-code diagnostics for `src/gui/`) <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/gui/panels.rs, src/gui/mod.rs, src/gui/physical.rs] -->

## 2. Merge the duplicate help view

- [x] 2.1 Replace `HelpView::Panels` and `HelpView::Physical` with a single `HelpView::ModuleUi` in `src/help.rs`: one variant, one title (`Module UI`), one keybinding table merging the two (module-UI navigation `j`/`k` and arrows, `Enter`/`Space`, wheel, `m`, `e`, `1`–`4`, `s`, `+`/`-`, plus `Tab`/`Shift+Tab`, `z`, `Alt+b`, `Alt+s`, `r`, and the global keys), and `active_view` reporting `ModuleUi` for the module-UI pane; update the `paint_help(..., HelpView::Panels)` call site in `src/gui/overlays.rs` so the tree compiles; keep the in-module `#[cfg(test)]` tests compiling (their parity refinement is task 3.1). Verify with `cargo test --lib help` green <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/help.rs, src/gui/overlays.rs] -->

## 3. Tests

- [x] 3.1 Update the help tests in `src/help.rs`: every `HelpView` has a non-empty keybinding table, the `ModuleUi` table names keys the handler actually dispatches (navigation, zoom, `s`, `g s`, `Tab`/`Shift+Tab`), and the retired Panels/Physical rows are gone; run `cargo test --lib help` green <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/help.rs] -->
- [x] 3.2 Add a regression in `src/regression.rs` asserting no Panels surface remains: no `gui::panels` module, no `PanelsFrame`/`handle_panels_frame`, and no `HelpView::Panels`/`HelpView::Physical` variant; run `cargo test --lib regression` green <!-- agent: horst-engineer.build, depends_on: [1.2, 2.1], touches: [src/regression.rs] -->

## 4. Verification

- [x] 4.1 Run the four gates and confirm each exits 0: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` <!-- agent: horst-engineer.fast, depends_on: [3.1, 3.2], touches: [] -->
