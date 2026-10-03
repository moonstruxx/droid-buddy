---
colors:
  # Classic palette (terminal and mono described in Theming). Every token resolves
  # through Theme::egui_color / Theme::egui_from_rgb; terminal maps most tokens to Reset.
  base:
    background: default
    foreground: white
    muted: dark-gray
    border: dark-gray
    accent: blue
  component:
    button: white
    switch: white
    knob: magenta
    cv-in: cyan
    cv-out: green
    led: red
    fader-led-bar: yellow
  shift:
    group-1: yellow
    group-2: cyan
    group-3: magenta
    group-4: green
  state:
    hover-background: dark-gray
    hover-emphasis: reversed
    dim: dim
    emphasis: bold
  picker:
    fav-file: light-yellow
    fav-dir: light-green
  viewer:
    key: cyan
    status-background: dark-gray
    focused-border: yellow
    occurrence: yellow
    current-occurrence: yellow-on-dark-gray
    boolean-modifier: cyan
    exact-value-modifier: magenta
    minimap-occurrence: yellow
    minimap-modifier-boolean: cyan
    minimap-modifier-exact: magenta
    minimap-combined: magenta
  graph:
    canvas-background: black
    node-fill: dark-gray
    node-border: white
    node-title: yellow
    node-highlight: yellow
    node-dim: gray
    node-controller: green
    node-jack-input: cyan
    node-jack-output: green
    cluster-border: blue
    cluster-title: blue
    edge-control: cyan
    edge-audio: green
    edge-midi: magenta
    edge-unknown: dark-gray
    edge-register: dark-gray
    edge-error: red
    edge-highlight: white
    edge-dim: dark-gray
    edge-diff-added: green
    edge-diff-removed: magenta
    edge-latency-0: blue
    edge-latency-1: cyan
    edge-latency-2: green
    edge-latency-3: yellow
    edge-latency-4: red
    edge-latency-legend: blue
  physical:
    skeleton-module-outline: white
  panes:
    focus-border: yellow
    unfocused-border: dark-gray
  validation:
    error: red
    warning: yellow
    hint: cyan
    modal-border: red
    selected-background: dark-gray
  optimizer:
    selected-background: dark-gray
    weight: yellow
  diff:
    added: green
    removed: magenta
typography:
  family:
    ui: egui-proportional
    code: egui-monospace
  size:
    ui-title: 12
    ui-body: 11
    ui-small: 9
    code: 13
    code-small: 11
  weight:
    emphasis: bold
    de-emphasis: dim
    hover: reversed
  line-height: 1.3
spacing:
  component-cell: 16x3-points
  header-height: 3
  status-height: 22-points
  main-min-height: 10
  panel-padding: 3
  pane-gap: 3
  picker-width-ratio: 0.7
  picker-height-ratio: 0.5
  viewer-sidebar-width-ratio: 0.2
  viewer-sidebar-min-width: 60
  viewer-status-height: 22
  viewer-minimap-width: 26
  graph-node: 22x5
  graph-cluster-padding: 2
  modal-width-ratio: 0.6
  modal-height-ratio: 0.7
  pane-split-default: 0.5
  pane-split-clamp: 0.3-0.7
  small-split-default: 0.5
elevation:
  level: none
motion:
  duration: none
  easing: none
radii:
  pane: 0
  modal: 8
  row: 2
shadows:
  shadow: none
---

# Design System

## Look and Feel

A flat, high-contrast native window with no decoration beyond thin borders and color: no gradients, no shadows, no animation. Visual hierarchy comes entirely from color and weight, with inverted emphasis on hover. The window is drawn with winit/egui/wgpu; the dark canvas reads as a single surface.

The interface reads like an instrument panel: the main view is a physical 1:1 faceplate model of the rack at millimeter scale, and each declared DROID controller (P2B8, Faderbank, Notebuttons, and more) is a bordered faceplate whose title names the controller, with every one of its interactive elements drawn at its faceplate position. The signal-flow graph shows the same circuits as a node network.

## Design Intent

- **Mirror the hardware.** Elements are grouped by physical controller and placed at their faceplate positions, so a user who knows the rack can find a control by where it physically lives. The physical module UI is the only hardware map; there is no separate compact panel representation.
- **State is always visible.** Every component shows its current state inline: buttons and switches show ON/OFF with filled/outline glyphs, knobs and encoders show a percentage, faders show a vertical track plus a bar in the `fader_led_bar` token, CV I/O shows direction.
- **Color is semantic, not decorative.** Each component kind has one color and each shift group has one color. The same color means the same thing everywhere it appears, and a theme swap recolors every surface at once.
- **Shift is a spotlight.** When a shift key (1-4) is held, elements of that shift group get a bold colored border with a `[SHIFT n]` marker; all other elements dim. The status bar repeats the active shift in its group color.
- **Modifier is a wash.** A modifier hardware token tints every influenced cell with a wash in `hash(token)%16` hue; unaffected cells dim. Pressing the mouse down on a component holds the token while the button stays down, while `m` on the hovered (or selected) component, Ctrl+Click, or Ctrl+Shift+Click latches it; the latch survives release until `Esc`. Exactly one modifier is effective at a time — a second latch replaces the first. The same hue tints source `select` spans and graph edges/nodes. Rendering priority is `graph_edge_error` (red) > modifier hue > cable kind; shift border and modifier wash coexist.
- **Interaction is forgiving.** Hover highlights the component; click toggles; scroll nudges values in small steps. Keyboard and mouse are interchangeable, and focus follows the clicked pane.

## Theming

Every color is a named semantic token resolved from the active theme at paint time through `egui_color` and `egui_from_rgb`; rendering code contains no raw literals. The token set covers component kinds (`button`, `switch`, `knob`, `cv_in`, `cv_out`, `led`, `fader_led_bar`), shift groups (`shift1`-`shift4`), chrome (`accent`, `muted`, `text`, `status_bg`), viewer keys and highlights (`viewer_key`, `focus_border`, `occurrence_highlight`, `modifier_boolean`, `modifier_exact`), the four minimap signal colors, pane borders (`pane_focus_border` and `pane_unfocused_border`), picker favourites (`picker_fav_file` and `picker_fav_dir`), the graph surface (node and cluster chrome, canvas background, node fill, edge kind colors, highlight and dim tokens, per-kind node frames for controllers and jacks, the register-edge token, diff colors, and the five-stop latency ramp), the physical skeleton outline, validation severity and modal chrome, and the optimizer selection and weight tokens.

Three built-in themes ship, selected by name (case-insensitive; `-`, `_`, and space are interchangeable separators):

| Theme | Character |
|---|---|
| `classic` | The original ANSI palette: kind colors white/magenta/cyan/green/red, shifts yellow/cyan/magenta/green, blue accents, dark-gray chrome, black graph canvas |
| `terminal` | Every token is `Reset` (resolved to neutral bright white) except the few that need contrast: the graph canvas stays black and the diff tokens stay distinct grays so added and removed cables remain tellable |
| `mono` | Grayscale only; shift tokens are pairwise distinct because shift groups are told apart by color alone |

The choice persists alongside label and latency settings. A missing file silently selects `classic` with sensible defaults; a malformed file or unknown theme name warns once at startup and falls back to `classic`. Per-patch labels live under the config directory keyed by canonicalized absolute patch path (`hw` per-token per-shift and `circuits` per node). The theme is installed before the window opens, so a session never renders with a half-selected palette.

## Main Band

The area below the header is a class-based pane layout. Every view belongs to a class: the signal-flow graph, the module UI, and the physical view are **big** (full-height) panes, while the source viewer and the optimizer are **small** (quarter-height) panes. The band is a full-height left big pane plus a right half that shows either one big pane or two stacked small panes — the right half splits into two small panes as soon as any small-class view is open, and collapses back to a single big pane when none is.

- **Startup**: the module UI occupies the left big pane and the source viewer a small pane on the right.
- **Focus**: exactly one pane owns the keyboard. `Tab` and `Shift+Tab` cycle focus forward and backward in tree order, skipping empty panes; clicking a pane focuses it. The focused pane draws the focus-border token, unfocused panes the unfocused-border token.
- **Keys**: `r` rotates the focused pane through its class carousel (big: graph, module UI, physical; small: source viewer, optimizer), focusing the view's existing pane when it is already open elsewhere. `z` maximizes the focused pane (cleared by `z` again, a focus change, or `Esc`). `Alt+b` and `Alt+s` swap the two big panes and the two small panes. `Esc` closes the focused view, clearing a maximize first.
- **Splits**: `[` and `]` move the left big-pane boundary in ten percent steps clamped to 30 to 70 percent; `Alt+[` and `Alt+]` move the small-pane boundary (and, on the graph pane, lower and raise cable tension). Both ratios persist across patch loads within a session.
- **Esc on the module UI**: with a shift or modifier active, `Esc` clears that state first and closes the pane otherwise.

## Component Anatomy

Each element occupies a compact cell. A scale factor cycles through 75 percent, 100 percent, 150 percent, and 200 percent with wrapping, reported as `Scaling: N%`; the 75 percent floor keeps module cells boxable. The module UI rescales cells with zoom; the published hit rects match what is drawn.

Every element renders as a compact two-line cell from one shared paint routine, so an element looks the same on every surface:

- **Row 1**: a state glyph, followed by the component label when the cell is wide enough (for example `● TRIG A`).
- **Row 2**: the state text (ON/OFF, percentage, CV direction) in muted gray, at a smaller proportional size. The row draws only when the cell is tall enough for both lines; short cells keep the glyph row alone.
- Fader-marked knobs and encoders skip the two rows and draw a bottom-up strip filled to the component's value in the `fader_led_bar` token.
- Over-long labels truncate with an ellipsis while cell geometry and hit rects stay unchanged.

LEDs are never drawn as standalone cells. A pure model resolver maps each LED token to its owning element — an explicit patch pairing first (a bare `led = L.N` entry, or a numbered `ledN = L.M` paired by numeric suffix with a same-suffix element entry such as `buttonN`, `potN`, `encoderN`, `switchN`, or `faderN`), then the controller's positional default (see the module UI section).

Glyphs by kind:

| Kind | On / value | Off / idle |
|---|---|---|
| Button | `●` | `○` |
| Switch | `▣` for ON, `◉` + percentage for a value | `□` |
| LED | `●` | `○` |
| Knob / Encoder | `◉` + percentage | `◉` + `---` |
| Fader | `▮` + track filled to value | `▮` + empty track |
| CV in | `◀` `CV IN` | `◀` `CV IN` |
| CV out | `▶` `CV OUT` | `▶` `CV OUT` |

## Module UI (Physical View)

The module UI is the hardware map: a physical 1:1 faceplate model of the rack, and the only surface that renders controllers. It draws every interactive element of every declared controller at its faceplate position, with a glyph for its element type (knob with pointer, vertical fader track with thumb, encoder, button, rotary switch, toggle switch, jack). Faceplate artwork is not rendered; the faceplate background is plain.

- **Geometry**: a millimeter-accurate grid model of the rack (case rows, fold bars, mount sections, module faceplates) mapped to screen points with aspect-compensated factors so physical proportions survive the non-square aspect.
- **Element fidelity**: each controller renders its real elements — an M4 fader is a vertical track with a thumb, an S10 shows two rotary and eight toggle switches, a B32 keeps its 4×8 button grid.
- **Unused elements**: elements the patch does not reference are still drawn, dimmed and unlabelled, because the geometry decides what exists, not the patch.
- **Repeated instances**: repeated controllers render as separate faceplates, each titled with the controller name and instance number (`P2B8 1`, `P2B8 2`).
- **LEDs fold into their element**: an LED belonging to an element renders as part of it and never as a standalone cell. The binding follows the controller's physical design — B32, P2B8, and P4B2 button LEDs light the button face; each M4 LED lights the touch plate below its fader; each E4 encoder carries a ring of 32 LEDs; P8S8 slider LEDs light the slider track; master, G8, and X7 LEDs render beside their jack. An explicit patch pairing overrides the positional default.
- **LED colour and brightness**: rendering reflects the brightness and colour the patch writes to the LED registers. RGB controllers (M4, E4, master, G8, DB8E) render the patch-set colour; white-only controllers (B32) render brightness alone; an LED with no colour written renders in the device default. Patch-driven LED colour is the one deliberate exception to theme-token-only colouring.
- **E4 rings**: each encoder's ring renders its own 32 LED registers (L1.1–L1.32 for the first, L1.33–L1.64 for the second, and so on), each segment showing that register's brightness and colour; the ring is never derived from the encoder's value.
- **Master faceplate**: master and master18 render as faceplates at the start of the rack with their 4×4 jack grid, LEDs beside the jacks, and button — input and output jacks distinguishable. There is no separate CV I/O group.
- **Labels**: every referenced element shows its shift-aware label through the existing resolver (stored label for the layer, then layer 1, then the patch preamble, then the derived name), updating immediately when the shift layer changes. A label too long for its space is ellipsized and the full label appears on hover and in the status bar. Source headers and graph node titles show the circuit label override when present. The label store, active shift layer, and layer config are seeded into `App` at startup, so painters resolve labels from app state and never re-read the config file per frame. The centered single-field edit overlay reuses the same modifier hue for its hint.
- **Fit-width default**: the view opens fitted to the width of its pane, on open and on patch load, using the published pane size. Zoom and pan remain available afterwards.
- **Shift**: with a shift active, elements of the active shift group get a bold border in the group color with a `[SHIFT n]` marker and all other elements dim.
- **Modifier wash**: with a modifier active, influenced cells render with a background wash in the modifier hue and unaffected cells dim slightly. The wash follows the held token while the mouse button is down and the latched token (`m`, Ctrl+Click) after release; `Esc` clears the latch. The wash is orthogonal to shift borders, so both coexist.
- **Interactions**: keyboard navigation between elements (`j`/`k` and the arrows when the rack fits), `Enter`/`Space` to toggle or select, the mouse wheel to adjust knobs and faders, the `m` latch, the `e` label editor, shift groups `1`–`4`, and the selected-circuit highlight all live here. `Esc` clears an active shift or modifier first and closes the pane otherwise.
- **Pane chrome**: the pane draws the shared focus border and the title `Module UI`.
- **Skeleton reference mode**: `s` swaps the full render for a pure geometry outline (case border, fold-bar dividers, mount regions, module frames, and element-cell markers) in the skeleton tokens. It is a presentation of the same layout, not a separate surface.
- **Zoom**: `+` and `-` cycle the presets 75 percent, 100 percent, 150 percent, and 200 percent with wrap-around; the status bar reports `Scaling: X%`. Zoom rescales the cells.
- **Pan**: arrow keys pan the rack when it overflows the pane and fall back to element navigation when it fits; the mouse wheel pans on overflow while a wheel over a knob or fader still adjusts its value when no overflow forces panning.
- **Rack definition**: the rack is an ordered list of rows plus optional mount sections with auto-pack and per-module row overrides; absent config keeps a single-row case wide enough for the whole chain. Auto-pack fills rows strictly top-to-bottom and always places a module wider than its row rather than dropping it.
- **Element state rendering**: each element renders its live state (buttons and switches with a glyph, knobs and encoders with a percentage, faders with a vertical track and a bar in the `fader_led_bar` token, CV I/O with direction). Adjoined element-cell hit rects are clamped at draw time so distinct cells never publish overlapping rects at any zoom preset.
- **Border abutment and switch placement**: adjacent module borders abut exactly at every zoom preset; switch cells place per the controller's geometry data and never collapse onto a neighboring control's cell when geometry lacks a matching switch cell.

## Status Bar

A dark-gray band at the bottom, bordered and left-aligned. It shows the current status message, appends `| SHIFT n ACTIVE` in the group color when a shift is active, appends `| MOD B1.1 → N cells / M cables` in the modifier hue when a modifier is active (both can coexist), and permanently displays the current display setting as `Scale`. It also surfaces transient hints such as cable tension and latency summaries.

## File Picker

An overlay centered in the window, roughly 70 percent of the width and 50 percent of the height, with a blue-bordered block titled `File Picker`. Entries are listed with a `▶` marker on the selected row. Directories and `.ini` files are selectable, other files are not. When not at the filesystem root, the parent-directory entry is the first entry rendered as `..`; at the root no `..` entry appears. Entries sort directories first, then `.ini` files. Favourited files and directories render in their distinct favourite tokens so the pinned section tells kinds apart at a glance.

## Source Viewer

Opened with `g` then `v`, the source viewer is a small-class pane: the header and status bands remain, the left big pane keeps the module UI, and the source viewer occupies a small pane on the right. An open file picker still has absolute precedence.

- **Adjustable split**: `[` and `]` move the left big-pane boundary in ten percent steps clamped to 30 to 70 percent, and `Alt+[` and `Alt+]` move the small-pane boundary. Both ratios persist across patch loads within a session.
- **Module UI pane**: the left big pane holds the module UI, with the focus token on its border while that pane is focused and the unfocused token otherwise.
- **Source pane**: internally split into a circuit sidebar, scrolling source content, and an optional minimap. The sidebar is about one fifth of the source-pane width with a minimum while retaining room for content. It is a blue-bordered `Circuits` block listing every section in parse order; repeated names are disambiguated as `copy`, `copy (1)`, `copy (2)`. The selected entry uses a muted backdrop; other entries are plain.
- **Focus emphasis**: the source content border and title use the focus token while that pane is focused and the unfocused token otherwise. `Tab` switches focus; `Esc` closes the viewer while preserving selection and source position.
- **Raw mode** (default): the content pane shows verbatim `.ini` lines, including comments and blank lines, with vertical scroll. The title is `Source [raw]`. `t` toggles to prettified mode without closing the viewer.
- **Prettified mode**: each circuit is rendered as a small box with a cap showing the circuit name in its kind color, one `key = value` line per setting with cyan keys and white values, and a base. Circuit frame colors reuse the component palette.
- **Selection highlights**: selected-token occurrences are yellow and bold; the current occurrence is yellow, bold, and reversed on dark gray. Boolean `select` spans are cyan, bold, and underlined; exact-value (`selectat`) spans are magenta, bold, and underlined. In prettified mode the same colors apply to values and token references.
- **Minimap**: when the loaded patch and window are wide enough, a `Map` column summarizes the full file. Plain lines use a muted dot; occurrence lines use a solid block in yellow; modifier lines use a shaded block in cyan for boolean or magenta for exact-value (combined lines are solid magenta). The visible viewport is shown as a reversed indicator and moves with source scroll. It hides when the window is too narrow.
- **Viewer status bar**: a dark-gray band with shortcut hints (`ESC` close, `j/k` scroll, `Up/Down` occurrence navigation, `Home/End` jumps, `t` toggle, `Tab` focus, `[ / ]` split) followed by any transient message so hints stay visible.
- **Empty states**: centered muted `No patch loaded` or `No circuits in patch` appears inside the source content border.
- **Live interaction**: the main window stays live while the viewer is open: toggles, shift groups, and scale work from either focus, and mouse clicks set focus to the clicked pane. Only conflicting navigation keys follow `Tab` focus.

## Signal-Flow Graph

Opened with `g` then `g`, the signal-flow graph is a big-class pane showing the patch's signal topology (circuits as nodes, virtual `_cable` connections as directed edges, and comment-banner groups as cluster containers). The header and status bands remain, and the rest of the band stays live alongside it.

- **Empty state**: with no patch loaded the surface shows the centered muted prompt `No patch loaded. Press 'l' to load.`
- **Clusters**: each banner group is a titled, plain-bordered container drawn as the padded union of its member nodes, so edges run behind the node frames. Members cohere toward the cluster centroid via a weak internal cohesion force so the group reads as a content container (force arrangement; the column arrangement draws the same container around its members without a cohesion force).
- **Edges**: cables render as polylines between ports, with the port areas covered by the node frames for a clean join. An edge's color is the cable kind of the producing circuit (declared `cable_kind` when present, otherwise name-substring inference): control in cyan, audio in green, midi in magenta, and unknown in dark gray, overridden by the red error token when a topology finding references the cable. Register edges (a circuit reading or writing a hardware register, directed by the catalog) render in the muted register token. Under an active select state, register edges whose circuit endpoint is classified `NotSelected` survive only when they reach a controller node. When the structural diff is shown, added or changed cables draw in the diff-added green and removed cables in the diff-removed magenta; error red still wins. Cable latency coloring (on by default, toggled by the `g c` prefix chord) replaces the kind color for non-error, non-diff cables with a blue to red ramp of five stops; back-edge cables always land on the hottest stop. Hovering a back-edge sink shows `reads _X 1 loop behind`.
- **Wiring-outlier detection**: a learned decision table classifies direct hardware bindings as implausible from binding features (distance, controller and rack flags, source and sink kind) with invariant guards applied at the call site and a preserved threshold fallback on a table miss. A second opinion z-scores each hardware token's influence size against per-kind corpus statistics and flags tokens beyond the band. Each finding is a warning topology finding so it lights the affected edge in red. Findings never block building or viewing the graph.
- **Nodes**: rounded frames titled with the circuit name (or the circuit label override when present, in both full and filtered panes); repeated names append the instance index. Circuit nodes use the white border and yellow title; controller nodes render in the controller token with a `P2B8 #1`-style title; input-jack and output-jack nodes render in their jack tokens with the register token as title. A left input port marks consumers and a right output port marks producers. A node whose circuit instance has processing disabled or whose section is classified `NotSelected` under an active select state renders dimmed; hover styling stays visible on dimmed nodes.
- **Layout arrangement**: the graph opens in a deterministic column arrangement by default: circuit nodes stack in dense-normalized depth columns (`0..N-1`) with width-aware, grid-snapped placement so nodes in a column never overlap; controller and input-jack nodes sit in a fixed left outer column and output jacks in a fixed right outer column. Within a column, nodes order by strict file order by default, or by crossing-minimization sweeps when `[layout] ordering = "barycenter"`. Pinned nodes hold their position as fixed anchors while the rest arrange around them. The force-directed solver is retained unchanged (spring convergence, tension, drag re-settle) and reachable via the `h` toggle on the focused graph pane (status `Layout: column` / `Layout: force`) or `[layout] mode = "force"`.
- **Interaction**: while the graph pane is focused it owns mouse input inside its slot. Dragging a node repositions it; on release the layout locally re-settles around the node while distant nodes stay anchored, and the node auto-pins at the dropped position so the placement survives. `x` toggles processing for the hovered circuit instance; `p` toggles pin and unpin on the hovered node (the first circuit in file order is pinned by default). `c` centers the graph in the visible pane (pan only, zoom unchanged) and `Shift+c` fits and centers it against the published pane size, resetting the zoom preset to the fit index. `+` and `-` cycle the persistent camera zoom presets (the list floors at `0.03125`, exactly 2× deeper zoom-out than the previous minimum) and arrow keys pan the camera on overflow. `Alt+[` and `Alt+]` lower and raise cable tension and re-solve the layout live; the status bar reports the value and determinism holds per tension value (tension tunes the force arrangement; on the column arrangement the value is stored and reported while positions stay put). `h` toggles the layout arrangement between the column default and the force solver, re-solving an open graph. `g s` opens the select-state overlay, `f` toggles the dependency filter on the hovered node (falling back to the shared circuit selection), and `Esc` clears the filter before it would close the slot. `Esc` closes the graph slot.
- **Minimap navigation**: when the graph overflows its pane, the corner minimap shows the whole layout with a viewport indicator. Left-clicking anywhere on the minimap pans the camera so the clicked world position becomes the viewport centre at the current zoom; the click is consumed and never falls through to node selection, drag, or marquee.
- **Canvas and window**: the graph renders as an egui canvas inside its pane, or across the whole window when the window is the graph surface. Nodes, edges, cluster containers, and labels are painted from the shared scene description under the shared camera; hit rects are derived from the same camera so pointer handling stays aligned. The scene paints from the pane rect and window input maps back through the pane origin, so the pane and the window share one pointer path. The paint paths publish the visible pane size as `graph_canvas_px` every frame (pane rect / window canvas size), and the first-frame fit seeds against that real size rather than a fixed viewport. Every color derives from the active theme's semantic tokens with the existing precedence (error red > diff > latency ramp > cable kind) unchanged.
- **Desktop graph window**: the window opens at startup and is the only surface; no in-session key opens or closes it. It paints the same scene under the shared camera, so node positions, edges, and colors match the embedded tile; every interaction maps onto the same mutations. Canvas polish includes middle-drag pan, cursor-anchored wheel zoom, marquee selection, a corner minimap with viewport indicator when the graph overflows, and a hover tooltip showing the circuit name and latency. Clicking a node sets the shared circuit selection so the source viewer jumps and the module UI highlights the same node. All window colors derive from theme tokens; switching the theme re-themes window and tile together. The window never opens under tests.

## Select State

`g s` opens a centered overlay listing the patch's discovered select signals (register signals from `select`-style params and cable signals from the cable index) each with its inferred candidate values. The overlay is centered, about 60 percent by 70 percent, clamped and rounded, with a border in the validation modal border token, titled `Select state (N)`, with the hint `j/k:navigate [/]:cycle Esc:clear`.

- **Cycle**: `j` and `k` move between signals; `[`, `]`, and `Enter` cycle the selected signal's candidate value, writing the assumed state and rebuilding the graph so sections reclassify live. `Esc` closes the menu and clears the state.
- **Classification**: every section is labeled `Selected` (its select param matches the assumed value), `NotSelected` (the assumed value excludes it), or `Unknown` (no select-param match, always kept). Selected sections keep their full edges; `NotSelected` sections dim and lose their controller register edges except the shared cross-controller pair. An optional `hide_unselected` mode additionally drops `NotSelected` circuit nodes.
- **Status**: each cycle updates the status bar to `Select state: N selected / M unselected / K unknown`. The state is in-memory per session with no persistence and no `.ini` mutation.

## Patch Diff

Opened with `g` then `d`, the patch diff loads a second patch through the file picker and highlights the structural difference from the currently loaded patch. It is a read-only overlay on top of the graph and source surfaces.

- **Trigger**: `g d` opens the picker in diff mode; picking a second patch computes the added, removed, and changed cables and nodes. `d` toggles the overlay; `Esc` clears a component-scoped filter first, then hides the overlay.
- **Graph surface**: with the diff shown, edges whose cable was added or changed render in the diff-added token and removed cables in the diff-removed token. Topology-error red still takes precedence over the diff colors, and the diff colors take precedence over the cable-kind inference. A scoped diff filters the report so the graph highlights and the status hint's cable count match.
- **Nodes**: added or removed circuit nodes are distinguished alongside the edge highlights.
- **Status**: a trailing hint reports the diff scope and cable count in the status bar.

## Patch Validation

Loading a patch runs a schema-driven validation pass. Findings are sorted by line and column and each carries a severity (error, warning, or hint) plus a diagnostic code and message with its source span.

- **Modal**: when the load produces findings, a centered modal opens (about 60 percent by 70 percent, clamped, rounded border in the validation modal border token, titled with the counts). Each row reads `L{line}:{col} [E|W|H] [code] message` with the severity bracket in the error, warning, or hint color, the location in the text color, the code in muted, and the message in the text color; the selected row is highlighted with the selected-background token and non-selected rows dim. A fixed bottom hint lists `e` toggle, `j/k` navigate, `Enter` jump, `Esc` close.
- **Responsive**: in narrow windows the modal shrinks to near full width, mirroring the picker and overlay responsiveness.
- **Scope**: validation findings never block building or viewing the graph; they are informational feedback shown on load, and the same error red lights offending cables in the graph.

## Circuit Plugins

Users can extend the circuit schema without rebuilding by dropping TOML plugin files into the plugins directory (or a configured override; loading can be disabled). Each plugin file contributes one or more `[[circuit]]` tables that merge over the embedded schema: a plugin circuit wins on a name collision with a single warning per file, and a file that is malformed or missing the required `ramsize` is skipped with one warning and never aborts startup.

- **Format**: a circuit table declares `name` (case-insensitive), `category`, the required `ramsize`, optional `title` and `description`, optional rendering metadata `cable_kind` and `color`, and `inputs` and `outputs` parameter arrays. Each parameter carries `name`, `short`, `type`, optional `default`, and the `prefix`, `count`, and `start_at` expansion triple, so numbered plugin params expand through the same path as embedded ones.
- **`cable_kind`**: optional enum `control`, `audio`, `midi`, or `unknown` (case-insensitive). When declared it overrides the name-substring inference for edges produced by that circuit; when absent inference applies exactly as before.
- **`color`**: optional component-kind theme token name. When declared the graph node for that circuit uses it; when absent name-inference applies.
- **Merge rule**: plugin files apply in sorted-filename order, insert-or-override on collision, with a single shadow warning per file. Neutral defaults are applied by the schema layer, not the plugin loader.
- **Rendering precedence**: declared metadata feeds the same precedence chain: error red > diff colors > modifier hue > cable kind (now declared or inferred) > dim for disabled instances. Plugin circuits participate in RAM overflow validation and the latency cost model via their declared `ramsize`, so a plugin circuit can never silently disable RAM validation.

## Optimizer

Opened with `g` then `o`, the optimizer is a small-class pane: it proposes reorderings of the patch's section blocks that reduce forward-loop latency (the extra loop units a consumer reads from a producer that runs later in the scan). Each candidate is a section permutation scored by a cost model whose per-circuit average latency is proportional to the circuit's RAM size (overridable per circuit). The objective is a weighted blend of the summed and worst forward-loop latency; the pane shows the current weight. The module UI remains visible in the left big pane while the optimizer pane is open; the pane border uses the focus tokens.

- **Candidates**: banner min-sum (permutes within each banner group; degenerates to the whole file when the patch has no banners), global min-sum, and min-max. The search opens with a cheap ranking pass that targets back edges first, scales large patches via multilevel coarsening with banner groups as hints and variable-neighborhood refinement, and adds simulated annealing with a seeded PRNG. Each candidate is shown with its `before -> after` average and maximum latency summary. The identity ordering is always among the candidates.
- **Keys**: `j` and `k` move the selection, `Enter` previews, `r` restores the original order, `s` exports, `[` and `]` adjust the objective weight (0.0 is pure min-sum, 1.0 is pure min-max, blended in between; the pane owns these keys while focused and the status reports the weight, with candidates regenerating live), `Esc` closes the pane (restoring the file order when a preview is active).
- **Preview**: applying a candidate reorders sections in place and rebuilds the graph, so the latency ramp recolors live and the status bar reads `Preview: <label>`. The original order is remembered and restored on demand.
- **Export**: `s` writes a reordered copy to `<stem>-latopt.ini` next to the source file via the lossless writer (atomic write with an auto-suffix on collision); the status bar reports the export path. The loaded patch is never mutated.

## Processing Pause

Pressing `p` toggles a global pause of the simulated processing. While paused, all module-UI content renders dimmed, the header shows a `PROCESSING PAUSED` marker, and component mutations are blocked until `p` is pressed again. Geometry is unchanged while paused, so mouse hit-testing keeps working. Pause state resets on patch load. On the graph surface `p` instead toggles pin and unpin on the hovered node; processing pause is reached with `p` on every other surface.

## Empty State

With no patch loaded, the main area shows the centered muted prompt `Press 'l' to load a patch`.

## Visual Validation Provenance

Face correctness is proven by headless egui assertions: each surface's paint routine is driven through a plain `egui::Context` and the emitted shape and label lists are asserted for geometry and theme colors. The per-surface shape and label tests live with the painting code and the token-resolution matrix lives with the theme.

<!-- Last updated: 2026-10-03 · class-based pane layout replaces the tiled band: every view has a Big or Small class, the band is a left big pane plus a right half of one big or two stacked small panes, and Tab/`r`/`z`/`Alt+b`/`Alt+s`/`Esc` plus the two split ratios drive it; the Panels view is retired and the physical module UI is the only hardware map (faceplate fidelity, unused elements dimmed, LEDs folded into their element with patch-driven colour, E4 rings, master faceplate at chain position 0, fit-width default, all Panels interactions moved onto it, pane chrome with the `Module UI` title); graph minimap click pans the camera; exactly one modifier is effective at a time; deterministic column graph layout by default with the force solver via `h`. Component anchors above document current behavior. -->
