# Tasks

## 1. Regression tests first (expected red)

- [x] 1.1 Add shift-tint tests (2 red member tests + 3 green guards) in `src/gui/physical.rs`: with Group2 active, a member button's spec carries the Group2 token (glyph colour and `shift_color` slot, composed ring base); a non-member spec is unchanged; with no shift every spec is byte-identical to today; unaddressed cells stay dim/`None` under shift. Verify they FAIL. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/gui/physical.rs] -->

## 2. Shift wiring

- [x] 2.1 Compute shift membership in `physical_spec` and feed both slots (`src/gui/physical.rs`): `is_shift_active` from `app.active_shift` vs `comp.shift_group` into `cell_visuals`, and `shift_color` set to the group token for members / `None` otherwise (never for unaddressed cells). Verify by making task 1.1 pass with `cargo test gui::physical::`. <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/gui/physical.rs] -->

## 3. Visual artifacts

- [x] 3.1 Add a `src/regression.rs` story (tint + no-op proof, 40 green) with an inline shift-group patch asserting member tint, non-member stability, and the no-shift byte-identical contract at spec level. Verify with `cargo test regression::`. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [2.1], touches: [src/regression.rs] -->
- [x] 3.2 Check `docs/assets/*.png` (seeded shift shot added; all others byte-identical) via `tools/screenshots/render.sh` (expect byte-identical without a seeded shift); add a seeded shift-active shot only if the harness can set `active_shift` reliably, else document the headless coverage and leave assets untouched. Verify with the render check. <!-- agent: layout-designer-engineer.build, depends_on: [2.1, 3.1], touches: [tools/screenshots/**, docs/assets/*.png] -->

## 4. Verification and closeout

- [x] 4.1 Run the full gate (fmt/clippy/1246+12+5 tests/release/screenshots green) (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`) plus the screenshot render check; every command exits 0. <!-- agent: horst-engineer.fast, depends_on: [2.1, 3.1, 3.2], touches: [] -->
- [x] 4.2 Close bead `droid_tui-cnb` (closed with change reference) with the change reference. Verify with `bd list --json`. <!-- agent: devops-engineer.fast, depends_on: [2.1], touches: [] -->
