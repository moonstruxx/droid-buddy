# OpenSpec Tasks: boxed-components-and-viewer-split

Wave plan (file-disjointness caps concurrency):
`[1.1, 1.2] → [3.1] → [3.2] → [4.1] → [5.1] → [6.1] → [6.2]`

---

## 1.1 Record `led =` association during parse

- [x] Add `led: Option<String>` field to `HwComponent` in `src/patch.rs`.
- [x] During section parsing, when a `led = L.N` assignment is found, link it to the element defined in the same section (`b =`, `e =`, `fader =`, etc.).
- [x] Add `fixtures/led_pairs.ini` with P2B8 buttons+LEDs and a knob without LED (mixed grid).
- [x] Add unit tests for LED association parsing (button with LED, section without led, existing parse unchanged).

<!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs, fixtures/led_pairs.ini] -->

## 1.2 Add viewer split ratio to App state

- [x] Add `viewer_split_ratio: f32` field to `App` in `src/app.rs`.
- [x] Add clamp helper: `adjust_viewer_split_ratio(delta: f32)` that adjusts and clamps to 0.3–0.7.
- [x] Default value: 0.6 (panels-favoring).
- [x] Do NOT reset on `load_patch` — ratio persists across loads (view preference).

<!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->

## 3.1 Boxed rendering for LED-associated elements

- [x] In `src/ui.rs`, render elements with `led: Some(...)` as one bordered box: kind-colored border, element symbol+label+state inside, LED glyph reflecting LED state.
- [x] Elements with `led: None` keep today's two-line text rendering.
- [x] Unify COMPONENT_HEIGHT from 2 to 3. Update grid math in `render_patch`.
- [x] Update `component_rects` hit-testing to use the new geometry.
- [x] The LED component is NOT rendered as its own standalone cell — skip it in the grid.

<!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/ui.rs] -->

## 3.2 Ratio-aware split in render_embedded_main

- [x] In `render_embedded_main`, use `app.viewer_split_ratio` to compute the panels vs source column widths.
- [x] Default: 60% panels / 40% source (ratio 0.6).
- [x] Enforce bounds: panels column min 30%, max 70%.

<!-- agent: layout-designer-engineer.build, depends_on: [1.2, 3.1], touches: [src/ui.rs] -->

## 4.1 Split-ratio keys + viewer status hints

- [x] Wire `[` and `]` keys in `handler.rs` to call `adjust_viewer_split_ratio(-0.1)` and `adjust_viewer_split_ratio(+0.1)` respectively, only when `showing_viewer` is true.
- [x] Update viewer status bar to mention `[` / `]` for split adjustment.
- [x] Update main status bar to show split ratio when viewer is open.

<!-- agent: rusty-engineer.build, depends_on: [3.2], touches: [src/handler.rs] -->

## 5.1 Regression suite

- [x] Boxed-cell rendering frames: LED-associated element renders as box, non-LED renders as text.
- [x] Mixed grid: boxed + text cells coexist correctly.
- [x] Click hit-testing on boxed cells toggles/selects the element.
- [x] Split ratio adjustment + clamping at bounds.
- [x] Narrow-terminal layout with boxed cells.
- [x] LED association parsing: button with LED, section without led.

<!-- agent: horst-engineer.build, depends_on: [4.1], touches: [src/app.rs, src/handler.rs, src/patch.rs, src/ui.rs, fixtures/led_pairs.ini] -->

## 6.1 Regenerate ARCHITECTURE.md + guardrails

- [x] Run `/make-architecture` to regenerate ARCHITECTURE.md.
- [x] Run `/make-guardrails` to update ob-guardrails-project.

<!-- agent: rusty-engineer.fast, depends_on: [5.1], touches: [ARCHITECTURE.md, .agents/skills/ob-guardrails-project/SKILL.md] -->

## 6.2 Regenerate DESIGN.md + verification gates

- [x] Run `/make-design` to regenerate DESIGN.md.
- [x] Run verification gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`.

<!-- agent: layout-designer-engineer.fast, depends_on: [5.1, 6.1], touches: [DESIGN.md] -->
