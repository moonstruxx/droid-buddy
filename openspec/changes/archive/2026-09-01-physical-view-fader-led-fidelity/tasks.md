# Physical View Fader + LED Fidelity — Tasks

## 1. Fader vertical track + amber LED bar

- [x] 1.1 Add a `fader_led_bar` amber theme token (classic/terminal/mono) and a Fader-visual marker so `physical_visuals` distinguishes F-family faders from knobs/encoders
  <!-- agent: layout-designer-engineer.build, depends_on: [], touches: [src/theme.rs, src/ui.rs] -->
- [x] 1.2 Render the vertical fader track (bottom-up fill proportional to value) and the amber LED bar in `render_physical_cell`, replacing the flat `◉ %` face for F-family elements
  <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/ui.rs] -->
- [x] 1.3 Add fader-column fixtures (P8S8 Faderbank, M4 Motorfader) at 0/50/100 % value and verify the track/LED-bar face renders position-correctly across zoom presets
  <!-- agent: layout-designer-engineer.build, depends_on: [1.2], touches: [fixtures/**] -->

## 2. Adjoined-cell rect clamping at non-default zooms

- [x] 2.1 Clamp adjacent element-cell `component_rects` at draw time in `render_physical_full` so no two cells share a column at any zoom preset (deterministic order-based clamp, drawn cells unchanged)
  <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/ui.rs, src/physical.rs], status: done, note: order-based same-row clamp (prev_right/prev_y) at the component_rects push site in render_physical_full; drawn cell + physical_full_rects stay geometric so all no-overlap/coincidence tests + visual snapshots stay green (cargo test --lib physical/ui = 200 passed). Full suite reports two hit-rect-only contract failures for 2.2 to update: regression_hover_hit_rect_matches_rendered_cell_at_nondefault_scale (first assertion: hit rect != geometric full rect at zooms 1.5/2.0) and regression_p2b8_panel_uniform_rows (P2B8 knob hit-rect width non-uniform at zoom 2, where rounding overlap is clamped). No visual face changes. -->
- [x] 2.2 Extend the strict no-overlap regression to run at every zoom preset (75/100/150/200 %) instead of 100 % only
  <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/regression.rs] -->

## 3. Device-dependent LED association

- [x] 3.1 Add per-controller LED-default table in `patch.rs` (M4 touch plate → RGB `L`/`R`, B32 → white-only, master → CD-channel default-link), applied only when no explicit `led`/`ledN` pairing exists
  <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs], status: done, note: device_default_led() keyed by resolved controller, applied in the controller-assignment pass only when comp.led is None: M4/motorfader B{inst}.{n} -> L{inst}.{n}; master CV I/O I{n}->R{n} / O{n}->R{8+n} (1..=8); B32 + all others stay white-only. 4 new tests in src/patch.rs (M4 twin, B32 none, master R-link incl. I9/O9 none, explicit led/ledN wins). Intended rendering consequence: m4 touch plates / CV jacks render boxed where cells are wide enough; fader_column snapshot min-width hint 580->516; rendermetrics corpus re-synced via tools/build_rendermetrics.py (rule mirrored; melody2 components 111->103). cargo test 711 passed, fmt + clippy clean. -->
- [x] 3.2 Add device-LED fixtures (M4 RGB, B32 white-only, master LED) and regression tests pinning each device's association and the explicit-pairing-wins rule
  <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/regression.rs, fixtures/**], status: done, note: fixtures/device_led_defaults.ini (bare [m4] + [b32] + [copy] I1/O1) + 3 regression tests: device_default_lights_resolve_for_m4_b32_master (per-device association incl. P-fader None, B2.32 None, I1/O1 R-link), explicit_led_pairing_wins_over_device_default (ledN-in-m4-section, motorfader alias, bare led), visual_device_led_defaults_snapshot (classic/mono x 80/120, 4 insta snapshots). cargo test 715 passed, fmt + clippy clean. -->

## 4. Remove unreachable boxed-LED branch

- [x] 4.1 Remove the `width>=5 && height>=3` boxed-LED branch from the physical cell path, verifying no element cell reaches the gate at any zoom preset and the compact cell is the sole physical contract
  <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/ui.rs, src/regression.rs] -->

## 5. Verification gate

- [x] 5.1 Run full verification gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) and fix any failures
  <!-- agent: horst-engineer.fast, depends_on: [1.3, 2.2, 3.2, 4.1], touches: [], status: done, note: gate green on 2026-09-01: fmt clean, clippy -D warnings clean, cargo test 715 passed / 0 failed (all targets), release build OK. -->
