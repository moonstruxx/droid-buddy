# Configuration

User config lives at `$XDG_CONFIG_HOME/droid-tui/config.toml`
(`~/.config/droid-tui/config.toml` by default). A missing file
silently yields defaults; a malformed file (or unknown theme) warns
once on stderr and falls back. Per-patch labels are separate — see
[Labels](labels.md#storage).

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

Module-UI presentation defaults (zoom, pan origin) plus the optional
rack/case row definition. Omit `[physical.rack]` for the auto-packed
default case. `+`/`-` adjust zoom at runtime.

### `[plugins]`

- `enabled` (bool, default `true`): user-supplied circuit definitions
  (`[[circuit]]` TOML files) extending the embedded schema.
- `dir`: optional **absolute** plugin-directory override. A
  non-absolute value is treated as unset (standard XDG plugins dir
  applies). Files load in sorted order; later files win on name
  collision (shadowing an embedded circuit warns once). A malformed
  file is skipped with a warning — startup never aborts.
