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
seed — plus the optional rack/case definition, including the modules each
row holds. Omit `[physical.rack]` for the auto-packed default case.

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

`top_mount_te` and `side_mount_te` give the case's top and side mount
sections in TE. Each `[[physical.rack.rows]]` entry is one row: `he` (row
height, 1 or 3), `hp` (width in HP), an optional `label`, and the optional
`modules` list of the modules that row physically holds.

```toml
[physical.rack]
top_mount_te = 0.0
side_mount_te = 0.0

[[physical.rack.rows]]
he = 3
hp = 84.0
label = "top"
modules = ["master", "P2B8 1", "P2B8 2", "G8", "R2C"]  # left to right

[[physical.rack.rows]]
he = 1
hp = 24.0
modules = ["X7"]        # a bare controller means instance 1

[physical.rack.assign]
"B32 1" = 1             # 0-based row index, for unlisted modules only
```

An entry in `modules` is a module key: the controller name the patch uses
plus the instance the patch numbers, counted from 1 (`P2B8 2`, `E4 1`). A
single-instance module needs no number, so a bare controller (`B32`, `G8`,
`X7`, `R2M`, `R2C`) means instance 1. `master`, `master18` and the patch
vocabulary `CV I/O` all name the master slot, so one inventory works
whichever master faceplate the patch resolves to. Controller names are
case-insensitive.

The list is rewritten on load and on save: entries are trimmed, empty and
unreadable ones (an instance of 0, a third field) dropped, and every
survivor re-emitted as `"{controller} {instance}"` with a lowercased
controller. An alias such as `CV I/O` is therefore stored as `master 1`,
and a saved file reads back unchanged. A controller the embedded geometry
does not know is handled later, at paint time: the entry is skipped after
one warning on stderr that names it, and the rest of the row still
renders. The warning appears once per distinct unknown name, not once per
frame.

The list also decides placement. A module named in a row sits in that
row, in list order, and `assign` cannot move it. `assign` still pins the
modules no row names, and the rest auto-pack in chain order. A master the
inventory never names is prepended, so the rack always holds exactly one.
Patch modules missing from every list are appended after the inventory in
the order the patch declares them, so a stale inventory never hides
hardware. With no `modules` in any row, the chain follows the patch and
the rows auto-pack, as before.

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
