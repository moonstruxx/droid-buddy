# Tasks: Fader + LED Contrast

- [x] 1.1 Exempt fader strip + LED dot from pause-dim (pass un-dimmed color for the lit strip fill and the LED dot fill; everything else keeps `paused`) <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/gui/physical.rs] -->
  <!-- Implemented directly by lead (worker model unavailable); LED dot was already undimmed, so only the strip changed. -->
- [x] 1.2 Add slot-outline + value-marker tokens and paint them (new `fader_slot` / `led_ring` tokens in classic/terminal/mono with per-palette tests; 1 px track outline + 1–2 px marker at fill height in `paint_fader`) <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/theme.rs, src/gui/physical.rs] -->
  <!-- Implemented directly by lead (worker model unavailable); both tokens added in one pass. -->
- [x] 2.1 LED ring + brightness floor (outline ring + inset core, intensity clamped to [0.25, 1.0], patch RGB keeps hue) <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/gui/physical.rs] -->
  <!-- Implemented directly by lead (worker model unavailable). -->
- [x] 3.1 Headless shape/label tests for the new chrome, full gates, live before/after at running + paused states <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/gui/physical.rs, src/theme.rs] -->
  <!-- Headless tests + fmt/clippy/full-suite/release green. Live before/after BLOCKED: X session degraded (no window opens; see droid_tui-533). -->
