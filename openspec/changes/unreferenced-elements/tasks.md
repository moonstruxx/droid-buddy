# Tasks

## 1. Regression tests first (expected red)

- [x] 1.1 Add enumeration tests in `src/physical.rs`: a module's unaddressed cells exclude addressed `(kind, element)` pairs, include ring segments and unused buttons, skip `element: None` cells, emit fallback modules empty, and order deterministically (family key, row, col across repeated runs). Verify they FAIL with no production change. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/physical.rs] -->
- [x] 1.2 Add emission tests (2 red + 2 guards; watchpoint: partial-B32 inline patch logs fallback-controller warning) in `src/gui/physical.rs`: a partial-B32 spec contains dimmed `None`-index cells for the unused buttons with no published hit rect; an unaddressed P2B8 LED emits no cell; a bare-E4 spec contains all 128 ring segments; a bare-P2B8 spec is unchanged (no unaddressed cells). Verify they FAIL. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/gui/physical.rs] -->

## 2. Emission and suppression

- [x] 2.1 Implement the pure unaddressed-cell enumeration (`unaddressed_cells`, notebuttons-shaped tests) in `src/physical.rs` (addressed-exclusion via `token_element`, deterministic family/row/col order) with unit tests. Verify by making task 1.1 pass with `cargo test physical::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/physical.rs] -->
- [x] 2.2 Emit unaddressed cells (master faceplate excluded; 1.2 test redirected to notebuttons) from `rack_geometry` and suppress stray fold-rule LEDs in `physical_spec` (`src/gui/physical.rs`), reusing the I/O `PortMark` mapping and the existing `None`/dimmed downstream path. Verify by making task 1.2 pass with `cargo test gui::physical::` (watch the performance-overlay paint tests for misplaced dimmed cells). <!-- agent: layout-designer-engineer.build, depends_on: [1.2, 2.1], touches: [src/gui/physical.rs] -->

## 3. Visual artifacts

- [x] 3.1 Add `fixtures/unreferenced_elements.ini` (151 dimmed-None count proves suppression) (partial B32 with 5 buttons, bare E4) and a `src/regression.rs` story asserting no `Error` finding, dimmed `None`-index cells for the 27 unused buttons and 124 ring segments, no standalone LED cell on fold-rule controllers, and no unaddressed token in the published hit rects. Verify with `cargo test regression::`. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [2.2], touches: [fixtures/unreferenced_elements.ini, src/regression.rs] -->
- [ ] 3.2 Re-bless `docs/assets/*.png` via `tools/screenshots/render.sh --bless` (the rotary-stack shot gains 124 dimmed ring segments; partial faces fill out) and verify the full check passes; confirm the dimmed elements read as quiet, not noisy. <!-- agent: layout-designer-engineer.build, depends_on: [2.2, 3.1], touches: [tools/screenshots/**, docs/assets/*.png] -->

## 4. Verification and closeout

- [ ] 4.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`) plus the screenshot render check; every command exits 0. <!-- agent: horst-engineer.fast, depends_on: [2.1, 2.2, 3.1, 3.2], touches: [] -->
- [x] 4.2 Close bead `droid_tui-xbm` (closed with change reference) with the change reference. Verify with `bd list --json`. <!-- agent: devops-engineer.fast, depends_on: [2.2], touches: [] -->
