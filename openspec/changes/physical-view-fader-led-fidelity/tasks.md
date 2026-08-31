# Physical View Fader + LED Fidelity — Tasks

## 1. Fader vertical track + amber LED bar

- [ ] 1.1 Add a `fader_led_bar` amber theme token (classic/terminal/mono) and a Fader-visual marker so `physical_visuals` distinguishes F-family faders from knobs/encoders
  <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/theme.rs, src/ui.rs] -->
- [ ] 1.2 Render the vertical fader track (bottom-up fill proportional to value) and the amber LED bar in `render_physical_cell`, replacing the flat `◉ %` face for F-family elements
  <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/ui.rs] -->
- [ ] 1.3 Add fader-column fixtures (P8S8 Faderbank, M4 Motorfader) at 0/50/100 % value and verify the track/LED-bar face renders position-correctly across zoom presets
  <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [fixtures/**] -->

## 2. Adjoined-cell rect clamping at non-default zooms

- [ ] 2.1 Clamp adjacent element-cell `component_rects` at draw time in `render_physical_full` so no two cells share a column at any zoom preset (deterministic order-based clamp, drawn cells unchanged)
  <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/ui.rs, src/physical.rs] -->
- [ ] 2.2 Extend the strict no-overlap regression to run at every zoom preset (75/100/150/200 %) instead of 100 % only
  <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/regression.rs] -->

## 3. Device-dependent LED association

- [ ] 3.1 Add per-controller LED-default table in `patch.rs` (M4 touch plate → RGB `L`/`R`, B32 → white-only, master → CD-channel default-link), applied only when no explicit `led`/`ledN` pairing exists
  <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs] -->
- [ ] 3.2 Add device-LED fixtures (M4 RGB, B32 white-only, master LED) and regression tests pinning each device's association and the explicit-pairing-wins rule
  <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/regression.rs, fixtures/**] -->

## 4. Remove unreachable boxed-LED branch

- [ ] 4.1 Remove the `width>=5 && height>=3` boxed-LED branch from the physical cell path, verifying no element cell reaches the gate at any zoom preset and the compact cell is the sole physical contract
  <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/ui.rs, src/regression.rs] -->

## 5. Verification gate

- [ ] 5.1 Run full verification gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) and fix any failures
  <!-- agent: horst-engineer.fast, depends_on: [1.3, 2.2, 3.2, 4.1], touches: [] -->
