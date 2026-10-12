# Tasks

## 1. Regression tests first (expected red)

- [x] 1.1 Add arrangement, position and ring regression tests in `src/physical.rs` (P2B8/P4B2/P10 big-pot and S10 rotary single-column stacks at ~24.8 mm pitch; P10 small pots still 2 cols × 4 rows; P2B8 button pitch 12.6 × 15.0; E4 four-encoder stack at 27.8 mm; DB8E encoder below its buttons; E4/DB8E ring cell extents) plus the `src/geometry.rs` cross-file test tying E4 to `rack_geometry.json`'s `e4` stack grid, and verify they FAIL against the current `controller_geometry.json` — do not relax an assertion to the old data <!-- agent: horst-engineer.build, depends_on: [], touches: [src/physical.rs, src/geometry.rs] -->

## 2. Geometry data

- [ ] 2.1 Rewrite the p2b8/p4b2/p10/s10/e4/db8e blocks in `tools/acquire_geometry.py` to the measured table in design.md (vertical stacks, measured pitches/sizes, DB8E encoder below the buttons), parameterize `ring_32` (2.76 mm pitch, 1.8 mm cells, 9 per side), switch the `grids`/`notes` provenance from "assumed" to measured, regenerate `controller_geometry.json` via `python3 tools/acquire_geometry.py`, and verify `--check` plus the task-1.1 tests pass <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [tools/acquire_geometry.py, controller_geometry.json] -->

## 3. Visual artifacts

- [x] 3.1 Add `fixtures/faceplate_rotary_stack.ini` covering all six corrected controllers (bare `[p2b8]`, `[p10]`, `[s10]`, `[e4]`, explicit p4b2 entries, db8e button and encoder entries) plus a `src/regression.rs` story that loads it, asserts no `Error`-severity validation finding, and asserts a faceplate for all six controllers with their declared tokens attributable — presence and attribution only, never positions (task 1.1 owns those) <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [], touches: [fixtures/faceplate_rotary_stack.ini, src/regression.rs] -->
- [ ] 3.2 Add the `faceplate-rotary-stack` shot to `tools/screenshots` (shot table + README row), re-bless `docs/assets/*.png`, and verify `render.sh` check-passes and that the P2B8 pot stack, the E4 encoder stack with its rings, and the DB8E encoder read correctly on the faceplate <!-- agent: layout-designer-engineer.build, depends_on: [2.1, 3.1], touches: [tools/screenshots/**, docs/assets/*.png] -->

## 4. Verification

- [ ] 4.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`), `python3 tools/acquire_geometry.py --check`, and `tools/screenshots/render.sh`, and verify every one exits 0 <!-- agent: horst-engineer.fast, depends_on: [2.1, 3.2], touches: [] -->
