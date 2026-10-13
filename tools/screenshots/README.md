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
| `unreferenced-elements.png` | `fixtures/unreferenced_elements.ini` | startup arrangement at 2x zoom (`setup_faceplate_stack`): module UI (left big) + source viewer (small); partial B32 (5 of 32 buttons, 27 dimmed) beside a bare E4 (124 dimmed ring segments) in one frame |
| `shift-colour.png` | `fixtures/button_led_composition.ini` | 2x stack zoom plus active Group2 (`setup_shift_colour`): module UI (left big) + source viewer (small); Group2 member faces tinted in the group token beside unshifted Group1/Group3 faces in one frame |
| `s10-switch-kinds.png` | `fixtures/s10_switch_kinds.ini` | startup arrangement at 2x zoom (`setup_faceplate_stack`): module UI (left big) + source viewer (small); bare S10 full front with rotary-round S1.1/S1.2 above toggle-square S1.3–S1.10 in one frame |
| `sections-lanes.png` | `fixtures/graph_topology_error.ini` | + signal-flow graph (right big) in `LayoutMode::Sections` (`setup_sections`): one patch-order lane row carrying both banner groups as contiguous bands |
| `sections-scale.png` | `fixtures/own_scale.ini` | same arrangement over the 509-section scale anchor: the width-aware lanes wrap into a ~1.34 aspect grid of rows instead of a 40:1 ribbon |

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

Resolved gap (change `unreferenced-elements`, task 3.2): the E4 32-LED rings
now render complete — unreferenced ring segments paint dimmed and unlabelled,
so the four dotted rings read around their encoders even though this fixture
declares no ring LED tokens. The DB8E ring likewise fills out wherever its
fixture leaves segments unreferenced. Ring geometry stays pinned by the
`src/physical.rs` tests.

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

`unreferenced-elements.png` is the visual check for change
`unreferenced-elements` (task 3.2): its fixture pairs a partial B32 (5 of 32
buttons via `[notebuttons]`) with a bare `[e4]`, so the 27 dimmed unlabelled
buttons and the 124 dimmed ring segments read beside their lit siblings in
one 1280x800 startup frame at 2x. All dimmed cells render the quiet `·`
marker with no label and no hit rect (stray fold-rule LEDs suppressed); the
harness renders static state only, so the shot proves the quiet unlit fill,
and the `src/regression.rs` story pins the counts instead.

`shift-colour.png` is the visual check for change `shift-colour-module-ui`
(task 3.2): the composition fixture's B32 buttons are Group2 members (shift
groups assign by controller number), so with Group2 active their composed
faces tint in the group token while the P2B8 (Group1) and M4 (Group3) faces
render exactly as unshifted. The seed is a plain `active_shift` field write
in `setup_shift_colour` before paint — deterministic, no input — and the
harness's double-render check enforces it. The no-shift no-op contract needs
no shot: every other render paints with `active_shift == None`, so their
byte-identical freshness is the proof.

`s10-switch-kinds.png` is the visual check for change `s10-switch-glyphs`
(task 3.2): a bare `[s10]` synthesizes the full 10-switch front whose S-family
geometry annotates S1.1/S1.2 `rotary` and the rest toggle, so the round
rotary glyphs read above the square toggle glyphs on one faceplate in a
1280x800 startup frame at 2x. Unannotated-switch faces keep the toggle glyph
by construction (the unannotated scenario), so no second shot is needed.

`sections-lanes.png` and `sections-scale.png` are the visual check for change
`sections-black-hole-solver` (task 7.1). Sections mode is a third graph
arrangement: circuits are laid out one per **lane** in patch order (lane width
= the widest member's width estimate, capped at `SECTIONS_LANE_MAX_NODE_W`),
lanes pack left→right and **wrap into rows** at `SECTIONS_ROW_TARGET_W`, and
the banner groups act as invisible anchors — a group reads as a contiguous
band of lanes, never as a packed box. The seed is a plain `layout_mode` field
write in `setup_sections` before the graph pane is placed, so the first solve
runs through `layout::solve_sections`; the harness's double-render check
enforces determinism.

`fixtures/graph_topology_error.ini` is the legible subject: it is the smallest
fixture carrying two real comment banners (Sources: `clocktool` + `divider`,
Sinks: `copy` + `p2b8`), so the shot shows what individual lanes look like at a
readable size — four labelled bodies in patch order with their cable springs
(including the red topology-error marker on the doubled `_CLK` edge), and both
banner groups as separate contiguous bands. On this fixture the lane row fits
inside the pane without wrapping (too few lanes to reach the row target), so
band contiguity is the claim this shot carries, not wraparound.

`fixtures/own_scale.ini` (509 sections, 6 banners) is the scale subject:
the same arrangement over the scale anchor, which used to solve into a 40:1
ribbon and now settles into a ~1.34 aspect grid. The shot shows the aspect fix
structurally — a stack of wrapped lane rows filling the pane width, the row
boundaries between them, the banner groups as contiguous row-bands (the blue
band rects), cable springs crossing lanes, and no overlapping node bodies.
At the fit zoom node labels are necessarily ellipsized (the whole 509-node
world is in frame), so this shot is evidence about layout shape, not about
text legibility; the legible-label claims live in `sections-lanes.png` and the
`src/layout.rs` sections tests.

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
tools/screenshots/render.sh --bless --only sections-lanes,sections-scale
```

A check fails when two fresh renders differ (nondeterministic paint) or when
a PNG on disk is stale (paint changed since the last bless). Re-bless, eyeball
the diff, commit the PNGs.

Build output goes to `/tmp/opencode/droid-screenshots-target` (or
`$CARGO_TARGET_DIR`), never into the repo. `rack_geometry.json` /
`controller_geometry.json` are symlinks: the app resolves them through
`CARGO_MANIFEST_DIR`, which is this directory at harness-compile time.
