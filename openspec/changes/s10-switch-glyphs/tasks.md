# Tasks

## 1. Regression tests first (expected red)

- [x] 1.1 Add loader tests (3 red + 1 guard) in `src/physical.rs`: a geometry entry with `"kind": "rotary"` loads into `ElementCellEntry.kind` and carries through to `ElementCell.kind`; entries without `kind` load `None`; M4 `touch` rides along. Verify they FAIL with no production change. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/physical.rs] -->
- [ ] 1.2 Add glyph tests in `src/gui/physical.rs`: a rotary-kind switch cell emits the round glyph set, a toggle-kind (and unannotated) switch the square set, with state text unchanged. Verify they FAIL. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/gui/physical.rs] -->

## 2. Kind carry and glyph branch

- [ ] 2.1 Add `kind: Option<String>` to `ElementCellEntry` and `ElementCell` and carry it in `preprocess_cells` (`src/physical.rs`). Verify by making task 1.1 pass with `cargo test physical::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/physical.rs] -->
- [ ] 2.2 Branch the Switch arm in `cell_visuals` on the geometry kind (`src/gui/physical.rs`): new `switch_kind: Option<&str>` param resolved at the spec site from the component's geometry cell (`None` at the label helper and test sites), rotary → round set, else toggle set. Verify by making task 1.2 pass with `cargo test gui::physical::`. <!-- agent: layout-designer-engineer.build, depends_on: [1.2, 2.1], touches: [src/gui/physical.rs] -->

## 3. Visual artifacts

- [ ] 3.1 Add `fixtures/s10_switch_kinds.ini` (S10 with rotary + toggle switches addressed) and a `src/regression.rs` story asserting no `Error` finding, rotary glyph on S1.1/S1.2 cells, toggle glyph on the rest, and unchanged state text. Verify with `cargo test regression::`. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [2.2], touches: [fixtures/s10_switch_kinds.ini, src/regression.rs] -->
- [ ] 3.2 Re-bless `docs/assets/*.png` via `tools/screenshots/render.sh --bless` (S10 faces change glyphs; all other shots must stay byte-identical) and verify the full check passes. <!-- agent: layout-designer-engineer.build, depends_on: [2.2, 3.1], touches: [tools/screenshots/**, docs/assets/*.png] -->

## 4. Verification and closeout

- [ ] 4.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`) plus the screenshot render check; every command exits 0. <!-- agent: horst-engineer.fast, depends_on: [2.1, 2.2, 3.1, 3.2], touches: [] -->
- [ ] 4.2 Close bead `droid_tui-a0f` with the change reference. Verify with `bd list --json`. <!-- agent: devops-engineer.fast, depends_on: [2.2], touches: [] -->
