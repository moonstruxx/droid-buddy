## 1. DB8E OLED placeholder

- [x] 1.1 Restore DB8E OLED upper-band placeholder in `src/gui/physical.rs::paint_physical` (derive B-grid top from `module.cells['B']`, state text via `physical::db8e_display_state_for_layout`) and verify `cargo test` passes
- [x] 1.2 Extend paint shape/label test to cover a db8e module (DB8E geometry with B cells + OLED band) and verify `cargo test` passes
- [x] 1.3 Run full verification gate `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` and verify all pass
