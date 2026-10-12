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
| `faceplate-rotary-stack.png` | `fixtures/faceplate_rotary_stack.ini` | startup: module UI (left big) + source viewer (small); all six corrected faceplates (P2B8, P4B2, P10, S10, E4, DB8E) in one frame |
| `button-led-composition.png` | `fixtures/button_led_composition.ini` | startup arrangement at 2x zoom (`setup_faceplate_stack`): module UI (left big) + source viewer (small); P2B8 + B32 round button faces, M4 touch-plate rectangles, and one LED-less pot in one frame |
| `led-register-colour.png` | `fixtures/led_register_colour.ini` | startup arrangement at 2x zoom (`setup_faceplate_stack`): module UI (left big) + source viewer (small); lit coloured P2B8 button face and M4 touch plate beside the dynamic-cable and sourceless white fallbacks in one frame |

Two more renders (`large-startup`, `large-graph` over the scale anchor
`fixtures/own_scale.ini`, 509 sections) are stability-only: hashed, compared,
never saved.

`faceplate-rotary-stack.png` is the visual check for change
`controller-geometry-fidelity` (task 3.2): its fixture declares all six
controllers whose faceplate geometry moved to the measured panel arrangement,
so every corrected element arrangement — the P2B8/P4B2/P10 pot stacks, the S10
rotary stack, the E4 encoder stack, and the DB8E encoder below its buttons —
reads in one 1280x800 frame. The module UI pane carries the startup
arrangement and is zoomed to 2x (`setup_faceplate_stack`) so the ~207 mm rack
of master plus six controllers fills the pane instead of rendering 38 px wide.

Known gap in this shot: the E4 and DB8E 32-LED rings do not appear. The
renderer paints a cell only for an element the patch declares, and this fixture
declares no ring LED tokens (a bare `[e4]` synthesizes four LEDs, which land on
the first four cells of ring 1), so the ring cells are unpainted and the
ring-legibility risk is not exercised by this shot. The ring geometry itself is
pinned by the `src/physical.rs` tests.

`button-led-composition.png` is the visual check for change
`button-led-composition` (task 4.2): its fixture declares a P2B8, a B32 and
an M4 with explicit button/LED pairs plus one LED-less pot, so every
composed face — the P2B8/B32 state circles with their dim floor-fill cores
and no standalone LED dots, the M4 touch-plate rectangle with the same
treatment — reads in one 1280x800 startup frame. All LEDs are `Off` at load
(the harness renders static state only: no selection, no input, no toggle
chord), so the shot proves the unlit faces plus the floor-fill core; the
lit fill is pinned by the `src/regression.rs` level tests instead.

`led-register-colour.png` is the visual check for change
`led-register-colour` (task 3.2): its fixture resolves two LEDs to static
colours at load (a P2B8 button LED and an M4 touch-plate LED, both at full
brightness via literal `ledcolor`/`ledvalue`), so the lit coloured faces —
the P2B8 state circle filled in its mapped colour, the M4 plate filled
likewise — read beside the two fallbacks (dynamic-cable `ledcolor` and the
plain pair with no colour params, both rendering the white floor fill) in
one 1280x800 startup frame at 2x. Only two controllers, so the stack zoom
fits with margin.

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
