---
colors:
  # Terminal ANSI palette (rendering depends on the user's terminal theme)
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
    encoder: magenta
    cv-in: cyan
    cv-out: green
    led: red
  shift:
    group-1: yellow
    group-2: cyan
    group-3: magenta
    group-4: green
  state:
    hover-background: dark-gray
    hover-modifier: reversed
    dim-modifier: dim
    emphasis-modifier: bold
  viewer:
    sidebar-border: blue
    content-border: dark-gray
    focused-border: yellow
    circuit-default-frame: blue
    entry-key: cyan
    entry-value: white
    occurrence: yellow
    current-occurrence: yellow-on-dark-gray
    boolean-modifier: cyan
    exact-value-modifier: magenta
    minimap-plain: dark-gray
    minimap-occurrence: yellow
    minimap-modifier: cyan-or-magenta
    minimap-viewport: reversed-on-dark-gray
    status-background: dark-gray
    shortcut-hint: cyan
  graph:
    node-border: white
    node-title: yellow
    port-input: cyan
    port-output: green
    cluster-border: blue
    cluster-title: blue
    edge-control: cyan
    edge-audio: green
    edge-midi: magenta
    edge-unknown: dark-gray
    edge-error: red
typography:
  family: terminal-default
  size: terminal-default
  modifiers:
    emphasis: bold
    de-emphasis: dim
    hover: reversed
spacing:
  component-width: 16
  component-height: 3
  header-height: 3
  status-height: 3
  main-min-height: 10
  panel-padding: 1
  picker-width-ratio: 0.7
  picker-height-ratio: 0.5
  viewer-sidebar-width-ratio: 0.2
  viewer-sidebar-min-width: 20
  viewer-status-height: 3
  graph-node-width: 22
  graph-node-height: 5
  graph-cluster-padding: 2
elevation:
  level: none
motion:
  duration: none
  easing: none
radii:
  radius: none
shadows:
  shadow: none
---

# Design System

## Look and Feel

A flat, high-contrast terminal interface built on the ANSI 16-color palette. The screen is divided into three fixed bands — a centered header, a flexible main area, and a status bar — with controller panels stacked vertically in the main area. There is no decoration beyond borders and color: no gradients, no shadows, no animation. Visual hierarchy comes entirely from color, weight (bold/dim), and the reversal of foreground and background for hover.

The interface reads like an instrument panel: each physical DROID controller (P2B8, Faderbank, Notebuttons, …) is a bordered box whose title names the controller, and inside it the hardware components sit in a fixed-width grid that mirrors their physical arrangement on the hardware.

## Design Intent

- **Mirror the hardware.** Components are grouped by physical controller and laid out in physical order (left-to-right, top-to-bottom), so a user who knows the rack can find a control by where it physically lives.
- **State is always visible.** Every component shows its current state inline: buttons and switches show ON/OFF with filled/outline glyphs, knobs and encoders show a percentage, CV I/O shows direction arrows, LEDs show a filled/outline dot.
- **Color is semantic, not decorative.** Each component kind has one color (knobs magenta, CV in cyan, CV out green, LEDs red) and each shift group has one color (1 yellow, 2 cyan, 3 magenta, 4 green). The same color means the same thing everywhere it appears.
- **Shift is a spotlight.** When a shift key (1–4) is held, panels containing that shift group get a bold colored border with a `[SHIFT n]` marker in the title; all other panels dim. The status bar repeats the active shift in its group color. The user always knows what a shift key will affect.
- **Modifier is a wash.** Selecting a modifier hardware token tints every influenced panel cell (boxed + text) with a background wash in `hash(token)%16` hue; unaffected cells dim. Source `select` spans and graph edges/nodes reuse the same hue; status shows `MOD B1.1 → N cells / M cables` in that hue. Rendering priority is `graph_edge_error` (red) > modifier hue > `CableKind`; shift border + modifier wash coexist.
- **Interaction is forgiving.** Hover reverses the component's colors; click toggles; scroll adjusts values in small steps (±0.05). Keyboard and mouse are interchangeable — the same component is targeted whether reached by `j`/`k` navigation or by pointing.

## Theming

Every color in the interface is a named semantic token resolved from the active theme at render time; rendering code contains no raw color literals. The token set covers component kinds (`button`, `knob`, `cv_in`, `cv_out`, `led`), shift groups (`shift1`–`shift4`), chrome (`accent`, `muted`, `text`, `status_bg`), viewer keys/hints (`viewer_key`), viewer highlights (`focus_border`, `occurrence_highlight`, `modifier_boolean`, `modifier_exact`), the four minimap signal colors (`minimap_occurrence`, `minimap_modifier_boolean`, `minimap_modifier_exact`, `minimap_combined`), and the graph surface tokens — node chrome (`graph_node_border`, `graph_node_title`), port markers (`graph_port_input`, `graph_port_output`), cluster chrome (`graph_cluster_border`, `graph_cluster_title`), and the five cable-edge colors (`graph_edge_control`, `graph_edge_audio`, `graph_edge_midi`, `graph_edge_unknown`, `graph_edge_error`).

Three built-in themes ship, selected by name (case-insensitive; `-`, `_`, and space are interchangeable separators):

| Theme | Character |
|---|---|
| `classic` | The original ANSI palette: kind colors white/magenta/cyan/green/red, shifts yellow/cyan/magenta/green, blue accents, dark-gray chrome |
| `terminal` | Every token is `Color::Reset`, so each user's terminal scheme supplies all colors — works with custom schemes and low-color terminals |
| `mono` | Grayscale only; shift tokens are pairwise distinct because shift groups are told apart by color alone during normal patching |

The choice persists in `$XDG_CONFIG_HOME/droid-tui/config.toml` as a single `theme = "…"` key. A missing file silently selects `classic`; a malformed file or unknown name warns once on stderr at startup and falls back to `classic`. The theme is installed before the terminal UI initializes, so a session never renders with a half-selected palette.

## Component Anatomy

Each component occupies a fixed cell of 16 columns × 3 rows. A scale factor is tracked through fixed presets of 50 %, 100 %, 150 % and 200 % (`+`/`-`, wrapping around at both ends) and reported in the status bar as `Scaling: N%`; it does not currently resize the rendered cells — components always render at 16×3 and the published hit rects match that fixed size.

Components without a parse-time LED association render as two-line text cells:

- **Row 1**: a state glyph followed by the component label (e.g. `● TRIG A`).
- **Row 2**: the state text (ON/OFF, percentage, CV IN/CV OUT), rendered in muted gray.

**Boxed cells for LED-associated components.** When a component's `.ini` section declares an LED association — a bare `led = L.N` entry, or a numbered circuit param `ledN = L.M` (e.g. `led11 = L1.1`) that shares its numeric suffix with a same-suffix element entry (`button11 = B1.1`, `pot11 = P1.1`) in the same section, the DROID convention for circuits like `matrixmixer` — the association is parsed into `HwComponent.led` and the component renders as one bordered box filling its full cell instead of a bare text cell:

- The box border uses the owning component's kind color — button/switch white, knob/encoder magenta, CV in cyan, CV out green, LED red.
- The element symbol + label live in the box's top title row, drawn inside the border row; the single interior row holds the element state text plus the LED glyph (`◉` lit, `○` unlit) reflecting the associated LED component's live state — one state, not a second textual LED state.
- Hover applies the same reversed/dark-gray emphasis to box content and border that text cells use.
- Components whose LED id does not resolve to an existing LED component fall back to the unlit glyph/state.
- LEDs referenced this way are never rendered as standalone grid cells; only unreferenced LEDs appear on their own.

Glyphs by kind:

| Kind | On / value | Off / idle |
|---|---|---|
| Button | `●` | `○` |
| Switch | `▣` | `□` |
| LED | `◉` | `○` |
| Knob / Encoder | `◉` + percentage | `◉` + `---` |
| CV in | `→` | `→` |
| CV out | `←` | `←` |

## Panels

- Each panel is a bordered block titled with the controller name (e.g. ` P2B8 `).
- A panel whose components come from more than one circuit instance is subdivided into per-instance module sub-blocks, each a bordered block titled with the controller name and instance number (e.g. ` P2B8 1 `, ` P2B8 2 `), stacked vertically; within a module, components flow left-to-right and wrap to additional rows at the panel's column width. A single-instance panel renders as one flat grid, and CV I/O is never subdivided.
- Panel layout follows the display orientation: panels stack vertically in portrait and arrange horizontally in landscape.
- Panel borders are dark gray by default.
- With a shift active: panels containing the active shift group get a bold border in the group color and a `[SHIFT n]` title marker; all other panels dim to dark gray.
- With a modifier active (e.g. `B1.1`→`_TRIG`): influenced cells (boxed LED-cells and text cells inside modules) render with a background wash in `hash(token)%16` modifier hue; unaffected cells dim slightly. Rendering priority is `graph_edge_error` (red) > modifier hue > `CableKind`. Modifier background wash is orthogonal to shift borders — both can coexist (yellow shift border + modifier hue cell bg, e.g. `SHIFT 1` + `B1.1`). Interaction: `Mouse Down` without mods = momentary preview (cleared on `Up`/`Leave`), `Ctrl+Shift+Click` (alias `Ctrl+Click`) = toggle latched, `m` = keyboard alias for hovered component, `Esc` clears shift + modifier; single-var today, additive `MOD B1.1+B1.2 → N cells / M cables` is aspirational.

## Status Bar

A dark-gray band at the bottom, bordered, left-aligned. It shows the current status message in white, appends ` | SHIFT n ACTIVE` in the group color, bold, when a shift is active, appends ` | MOD B1.1 → N cells / M cables` in the modifier hue (bold) when a modifier is active (both can coexist), and permanently displays the current display settings as `Scale: <factor> | Orientation: <Portrait|Landscape>`. Mouse `Down` = momentary modifier preview, `Ctrl+Shift+Click` (or `Ctrl+Click`) = toggle latched, `m` = keyboard alias, `Esc` clears shift + modifier.

## File Picker

An overlay centered in the terminal, roughly 70% of the width and 50% of the height, with a blue-bordered block titled ` File Picker `. Entries are listed with a `▶` marker on the selected row. The picker is a functional browser: directories and `.ini` files are selectable, other files are not.

## Source Viewer

Opened with `g` then `v`, the source viewer is embedded in the existing three-band TUI. The header and bottom status band remain; the flexible main band becomes a horizontal panels|source split. An open file picker still has absolute precedence and renders over the viewer.

- **Adjustable split**: while the source pane is open, `[` narrows the source pane and `]` widens it in ±10 % steps, clamped to 30–70 % of the main band (default 60/40 favoring panels). The ratio persists across patch loads within a session.

- **Panels pane**: a bordered ` Panels ` block containing the normal hardware layout. Its border is bold yellow when panel focus is active; otherwise it is dark gray. Panels stay interactive while the viewer is open: toggles, shift groups, scale, and orientation work from either focus.
- **Source pane**: internally split into a circuit sidebar, scrolling source content, and an optional minimap. The sidebar is ~1/5 of the source-pane width, with a 20-column minimum while retaining at least 20 columns for content. It is a blue-bordered ` Circuits ` block listing every `[section]` in parse order; repeated names are disambiguated as `copy`, `copy (1)`, `copy (2)`. The selected entry uses reversed video; other entries are white.
- **Focus emphasis**: the source content border and title are bold yellow while source focus is active, and dark gray while panel focus is active. `Tab` switches focus; `Esc` closes the viewer while preserving selection and source position.
- **Raw mode** (default): the content pane shows retained verbatim `.ini` lines, including comments and blank lines, with vertical scroll. The title is ` Source [raw] `. `t` toggles to prettified mode without closing the viewer.
- **Prettified mode**: each circuit is rendered as a small ASCII box — a `┌─ name ─┐` cap with a bold, circuit-colored name, one `│ key = value │` line per setting with cyan keys and white values, a `└────┘` base, and a blank line. Circuit frame colors reuse the component palette: buttons/switches/notebuttons white, pots/encoders/faderbank magenta, CV in cyan, CV out green, LEDs red, and unknown circuits blue.
- **Selection highlights**: selected-token occurrences are yellow and bold; the current occurrence is yellow, bold, and reversed on dark gray. Affected boolean `select`/transitive modifier spans are cyan, bold, and underlined. Affected exact-value (`selectat`) spans are magenta, bold, and underlined. In prettified mode, affected values use the same cyan/magenta modifier colors; token references are yellow, bold, and reversed. Clearing selection clears these highlights.
- **Minimap**: when the loaded patch and terminal are wide enough, a ` Map ` column is 3 columns wide (plus borders) and summarizes the full raw file. Plain lines use dark-gray `·`; occurrence lines use yellow `█`; modifier lines use cyan `▓` for boolean or magenta `▓` for exact-value relationships (combined occurrence/modifier lines are magenta `█`). The visible source viewport is shown as a reversed dark-gray indicator and moves proportionally with source scroll. The renderer publishes the minimap rectangle for click-to-scroll hit testing. It is hidden when total width is below 80 columns, the source pane is below 40 columns, height is below 10 rows, or keeping it would reduce readable source content below 20 columns.
- **Viewer status bar** (bottom, 3 rows): a dark-gray bordered band reads bold-white `Source Viewer | ` followed by cyan shortcut tokens — `ESC` close, `j/k` scroll, `Up/Down` occurrence navigation, `Home/End` first/last occurrence, `t` mode toggle, `Tab` focus, and `[ / ]` split adjustment. Transient messages (e.g. `Panels/Source split: 50%/50%`) render as trailing spans *after* the hint list so the hints always stay fully visible.
- **Empty states**: centered muted `No patch loaded` or `No circuits in patch` appears inside the source content border; the sidebar remains an empty bordered block.
- **Live interaction**: the source pane never toggles components directly, but the main window stays live while it is open: Enter/Space/click toggle+select components (selection re-jumps the source view), shift/scale/orientation keys work from either focus, and mouse clicks set focus to the clicked pane (component → Panels, source-pane area → Source). Only conflicting navigation keys (`j`/`k`, arrows) follow `Tab` focus.

## Signal-Flow Graph

Opened with `g` then `g` (mirroring the source viewer's `g v`), the signal-flow graph is embedded in the existing three-band TUI: the header and status bands remain, and the main band becomes a full-screen surface for the patch's signal topology — circuits as nodes, virtual `_cable` connections as directed edges, and comment-banner groups as cluster containers. An open file picker still has absolute precedence.

- **Empty state**: with no patch loaded the surface shows the centered muted prompt `No patch loaded. Press 'l' to load.`
- **Clusters**: each `# ---- Name ----` banner group is a titled, plain-bordered container (blue border and title) drawn as the padded union of its member nodes' rectangles, so edges run behind the node frames. The renderer publishes each cluster rect for hit testing.
- **Edges**: cables render as box-drawing polylines (`│ ─ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`) drawn cell by cell between the ports, with the port cells covered by the node frames for a clean join. An edge's color is the cable kind inferred from the producing circuit's name — **control** (circuits named clock/gate/trigger/pulsar/div) cyan, **audio** (the default) green, **midi** (midi/note/seq/pitch) magenta, and **unknown** dark gray when no edge produces the cable — overridden by the red `graph_edge_error` token when a topology-validation finding (dangling sink, `n → 1`) references the cable.
- **Nodes**: ComfyUI-style rounded frames (`BorderType::Rounded`), 22×5 cells, titled with the circuit name; repeated names append the instance index (`copy`, `copy (1)`, `copy (2)`). The node border is white and the title yellow. A left `◉` input port marks nodes that consume cables; a right `●` output port marks producers (presence markers, not per-parameter pairing).
- **Interaction**: while the surface is open it owns all mouse input. Left-dragging a node repositions it; on release the layout locally re-settles around the node (damped, bounded iteration budget) while distant nodes stay anchored. `Esc` closes the surface and restores the prior view; `q`/Ctrl+C still quit and `l` still opens the picker. The status bar continues to show the scale/orientation/shift state.

## Empty State

With no patch loaded, the main area shows the centered muted prompt `Press 'l' to load a patch`.

## Visual Validation Provenance

Face correctness is proven via the `insta` snapshot harness in `src/regression.rs` (`buffer_to_ansi` trims trailing empty cells, `buffer_to_html` maps fg/bg/bold/dim/reversed per span). The browsable gallery at `evidence/gallery/index.html` renders one row per scenario — fixtures `arpeggio1.ini`, `led_pairs.ini`, `source_navigation.ini`, `multi_module_p2b8.ini`, `numbered_led_pairs.ini` × themes `classic`/`terminal`/`mono` × widths 80/120 and viewer open/closed + shift1 — as HTML + ANSI sidecars. Graph-surface faces are covered by the same snapshot harness (scenarios `cable_banner_combos.ini`, `graph_edge_kinds.ini`, `graph_topology_error.ini` × `classic`/`mono` × widths 40/100), asserting cluster/node frames, edge-kind colors, and the topology-error highlight. Output is ephemeral in the worktree (`.gitignore`'d, generated via `cargo run --bin snapshot-gallery` or `cargo test -- --generate-gallery`) and durable in the OpenSpec archive (`scripts/archive-gallery.sh` mirrors into `openspec/changes/archive/2026-08-24-add-visual-validation/evidence/gallery`); the strict gate (`cargo test` / `cargo insta test --check`) fails on any face mismatch. This is the `visual-validation` change; see `openspec/specs/visual-validation/spec.md`.

<!-- Last updated: 2026-08-25 · signal-flow-graph surface: new Signal-Flow Graph section (clusters/edges/nodes/interaction), graph color tokens in frontmatter + Theming, graph node/cluster spacing tokens, graph snapshot scenarios in Visual Validation -->
