# Tasks

## 1. Regression tests first (expected red)

- [x] 1.1 Add resolver regression tests in `src/patch.rs`: a literal `ledcolor`/`ledvalue` pair on a motorfader section resolves to the mapped rgb at the given value; a constant expression folds; a single-hop cable ref to a constant producer output resolves; a dynamic cable ref yields `None`; an LED with no source keeps `Off`; B32 drops a resolved colour (brightness only). Verify they FAIL with no production change. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/patch.rs] -->
- [x] 1.2 Add toggle-preservation tests in `src/handler.rs`: toggling a `Led { value, rgb }` component alternates value 0.0/1.0 with `rgb` intact across `Enter`, `Space`, and `Ctrl+Enter`; plain `Off`/`On` LEDs keep the old arms. Verify they FAIL. <!-- agent: horst-engineer.build, depends_on: [], touches: [src/handler.rs] -->
- [x] 1.3 Add a painter test (gate lint fixed: contains) in `src/gui/physical.rs`: a composed cell whose folded LED carries `Led { value: 1.0, rgb: Some([r,g,b]) }` emits the fill in that rgb (not white). Verify it FAILS (fill renders white today). <!-- agent: horst-engineer.build, depends_on: [], touches: [src/gui/physical.rs] -->

## 2. Resolution and preservation

- [x] 2.1 Implement the static colour/brightness resolver (fixed on re-wave; gate lint fixed: redundant_closure) dynamic colour must poison (return None, not fall through to rank-2) and rank-2 must not match sections that never mention the element/LED poison-on-dynamic + rank-2 dropped; 3.1 story green) in `src/patch.rs` (same-suffix `ledcolorN`/`ledvalueN` pairing, single-`ledcolor` fallback, literal/const-expr/single-hop-cable resolution via `expression.rs`, hue table with corpus citation) and wire it into LED initial state at load (upgrade to `Led { value, rgb }` only when a source resolves; B32 drops colour). Verify by making task 1.1 pass with `cargo test patch::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs] -->
- [x] 2.2 Preserve `rgb` in `toggle_component` (`src/handler.rs`): `Led { v, rgb }` alternates value 0.0/1.0 keeping `rgb`; other states keep the old arms. Verify by making task 1.2 pass with `cargo test handler::`. <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/handler.rs] -->

## 3. Visual artifacts

- [x] 3.1 Add `fixtures/led_register_colour.ini` (P2B8 button with literal `ledcolor` + lit `ledvalue`, M4 touch plate with literal colour, one dynamic-cable LED for the fallback, one uncoloured LED) and a `src/regression.rs` story asserting no `Error` finding, resolved `Led { value, rgb }` states where expected, `Off` where not, and no folded LED published as a cell. Verify with `cargo test regression::`. <!-- agent: dermannmitdermachine-engineer.fast, depends_on: [2.1], touches: [fixtures/led_register_colour.ini, src/regression.rs] -->
- [x] 3.2 Re-bless `docs/assets/*.png` (new led-register-colour shot; existing PNGs byte-identical — their fixtures carry no colour sources) via `tools/screenshots/render.sh --bless` (the lit coloured faces change pixels) and verify the full check passes; confirm the new coloured faces read correctly on the module-ui shot. <!-- agent: layout-designer-engineer.build, depends_on: [2.1, 2.2, 3.1], touches: [tools/screenshots/**, docs/assets/*.png] -->

## 4. Verification and closeout

- [ ] 4.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`) plus the screenshot render check; every command exits 0. <!-- agent: horst-engineer.fast, depends_on: [2.1, 2.2, 3.1, 3.2], touches: [] -->
- [x] 4.2 Close bead `droid_tui-2n4` (closed; follow-up `droid_tui-3wz` filed) with the change reference and file the hue-table hardware-verification follow-up bead (`discovered-from`, type task, P3). Verify with `bd list --json`. <!-- agent: devops-engineer.fast, depends_on: [2.1, 2.2], touches: [] -->
