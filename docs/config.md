# Configuration

User config lives at `config.toml` in the `droid-tui` data directory:
`$XDG_CONFIG_HOME/droid-tui/` when that is an absolute path, else
`~/.config/droid-tui/` on Linux and macOS, and `%APPDATA%\droid-tui\`
on Windows. A missing file silently yields defaults; a malformed file
(or unknown theme) warns once on stderr and falls back. Per-patch
labels are separate — see [Labels](labels.md#storage). [Platforms →
Where your files live](platforms.md#where-your-files-live) has the
resolver rules and the directory's other files.

```toml
theme = "classic"

[labels]
layers_enabled = true
max_shift_layer = 4

[latency]
# per_circuit = { vco = 1.5 }

[layout]
mode = "column"      # or "force"
ordering = "strict"  # or "barycenter"

[physical]
labels = "auto"      # off / auto / on
zoom = 1.0
offset_x = 0.0
offset_y = 0.0
# [physical.rack] defines custom rows; omit for auto-pack.

[plugins]
enabled = true
# dir = "/absolute/path/to/plugins"  # absolute only; relative is ignored
```

## Keys

### `theme`

Palette name (`classic`, `terminal`, `mono`, …). Every rendered color
comes from theme tokens — no hardcoded RGB.

### `[labels]`

- `layers_enabled` (bool, default `true`): per-shift-group label
  layers. Off coerces display to layer 1 while preserving 2–N.
- `max_shift_layer` (1–8, default `4`): how many layers the `e`
  overlay cycles with `1`–`N`. Clamped on load and on save.

### `[latency]`

- `per_circuit` (map of lowercased circuit name → average-latency
  cost, default empty): overrides the ramsize-proportional heuristic.
  Shared by the graph's latency coloring (`g c`) and the
  [optimizer](views.md#latency-optimizer-g-o).

### `[layout]`

- `mode` (`column` default / `force`): the graph arrangement. `h` on
  the graph pane toggles at runtime.
- `ordering` (`strict` default / `barycenter`): within-column node
  order for the column arrangement.

### `[physical]` / `[physical.rack]`

Module-UI presentation defaults — the label presentation and a zoom/pan
seed — plus the optional rack/case row definition. Omit `[physical.rack]`
for the auto-packed default case.

- `labels` (`auto` default, `off`, `on`): how labels reach you. `off`
  draws them inside their cells only. `on` also annotates every addressed
  element that carries a written label, outside the rack with a leader
  line. `auto` annotates exactly the written labels whose cell cannot draw
  them whole at the current zoom; because the default view fits every
  written label into its cell, that set is normally empty, so `auto` is
  silent on a patch whose labels fit and only speaks for a label no
  supported zoom can hold (a long label on a 3 mm LED, say). An unknown
  value warns once and falls back to `auto`.
- `zoom` (clamped to 0.25–4.0, default `1.0`): the zoom seed, where `1.0`
  is true size (one millimetre is 3.7795 pt). The view fits itself on its
  first frame, so the opening zoom is the fit's rather than the seed's;
  `+`/`-` step the presets and the wheel zooms at runtime.
- `offset_x` / `offset_y` (points, default `0.0`): the pan seed, replaced
  on that same first frame by the fit's centring.

The view opens at the **labels fit**: the greater of the whole-rack fit
and the zoom every written label needs to be drawn whole, clamped to the
supported range, so a dense rack opens zoomed past the pane and pans.
`f` re-applies it and `Shift+F` applies the **rack fit** instead — the
whole rack in the pane, labels shrinking away where cells become too
small. The status line names which ran (`fit(labels)` / `fit(rack)`).
Neither the fit nor its mode is written back to this file.

### `[plugins]`

- `enabled` (bool, default `true`): user-supplied circuit definitions
  (`[[circuit]]` TOML files) extending the embedded schema.
- `dir`: optional **absolute** plugin-directory override. A
  non-absolute value is treated as unset (the plugins directory inside
  the platform data dir applies). Files load in sorted order; later
  files win on name collision (shadowing an embedded circuit warns
  once). A malformed file is skipped with a warning — startup never
  aborts.
