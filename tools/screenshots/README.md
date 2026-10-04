# Screenshot harness (task 1.1, change `readme-docs`)

Deterministic `docs/assets/*.png` renders for the README/manual, painted
through the app's real egui dispatch — headless, no window, no hand captures.

## Shots

| PNG | Fixture | Arrangement |
|---|---|---|
| `module-ui.png` | `fixtures/own_buttons.ini` | startup: module UI (left big) + source viewer (small) |
| `graph.png` | `fixtures/own_buttons.ini` | + signal-flow graph (right big) |
| `source-viewer.png` | `fixtures/source_navigation.ini` | source viewer maximized |
| `optimizer.png` | `fixtures/optimizer_latency.ini` | + latency optimizer (small) |

Two more renders (`large-startup`, `large-graph` over the scale anchor
`fixtures/own_scale.ini`, 509 sections) are stability-only: hashed, compared,
never saved.

## How it works

`src/main.rs` compiles the app sources in place (path-shadowed modules, the
`gui` paint module textually included) and drives `paint_panes` /
`paint_overlays` inside an `egui_kittest` harness with a headless wgpu
renderer at a fixed 1280x800 viewport. Fixed classic theme, isolated empty
XDG dir, three warmup frames (graph camera seeds from published geometry),
settle animation cancelled so captures show solved positions.

## Usage

```sh
tools/screenshots/render.sh            # check: render 2x, compare, diff vs disk
tools/screenshots/render.sh --bless    # rewrite docs/assets/*.png
tools/screenshots/render.sh --only graph,optimizer
```

A check fails when two fresh renders differ (nondeterministic paint) or when
a PNG on disk is stale (paint changed since the last bless). Re-bless, eyeball
the diff, commit the PNGs.

Build output goes to `/tmp/opencode/droid-screenshots-target` (or
`$CARGO_TARGET_DIR`), never into the repo. `rack_geometry.json` /
`controller_geometry.json` are symlinks: the app resolves them through
`CARGO_MANIFEST_DIR`, which is this directory at harness-compile time.
