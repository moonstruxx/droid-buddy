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
    node-dim: gray
    edge-dim: dark-gray
    edge-diff-added: green
    edge-diff-removed: magenta
    edge-latency-0: blue
    edge-latency-1: cyan
    edge-latency-2: green
    edge-latency-3: yellow
    edge-latency-4: red
    edge-latency-legend: blue
  diff:
    added: green
    removed: magenta
  validation:
    error: red
    warning: yellow
    hint: cyan
    modal-border: red
    selected-bg: dark-gray
  optimizer:
    modal-border: blue
    selected-bg: dark-gray
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

Every color in the interface is a named semantic token resolved from the active theme at render time; rendering code contains no raw color literals. The token set covers component kinds (`button`, `knob`, `cv_in`, `cv_out`, `led`), shift groups (`shift1`–`shift4`), chrome (`accent`, `muted`, `text`, `status_bg`), viewer keys/hints (`viewer_key`), viewer highlights (`focus_border`, `occurrence_highlight`, `modifier_boolean`, `modifier_exact`), the four minimap signal colors (`minimap_occurrence`, `minimap_modifier_boolean`, `minimap_modifier_exact`, `minimap_combined`), the graph surface tokens — node chrome (`graph_node_border`, `graph_node_title`), port markers (`graph_port_input`, `graph_port_output`), cluster chrome (`graph_cluster_border`, `graph_cluster_title`), the five cable-edge colors (`graph_edge_control`, `graph_edge_audio`, `graph_edge_midi`, `graph_edge_unknown`, `graph_edge_error`) — the latency ramp (`graph_edge_latency_0`–`_4` blue→red, with the mono variant a grayscale ramp and terminal all-Reset) plus `graph_edge_latency_legend` — the structural-diff cable colors (`graph_edge_diff_added` green, `graph_edge_diff_removed` magenta) — the validation severity tokens (`validation_error` red, `validation_warning` yellow, `validation_hint` cyan) plus modal chrome (`validation_modal_border`, `validation_selected_bg`), the render-outlier token (`render_outlier_warning` — classic yellow, mono white, terminal reset) for the predicted-degraded render status hint — the optimizer menu chrome (`optimizer_modal_border` blue, `optimizer_selected_bg` dark gray) — the physical-skeleton tokens (`physical_skeleton_module_outline` — classic white, terminal reset, mono light gray; `physical_skeleton_cell` cyan; `physical_skeleton_port_in` cyan; `physical_skeleton_port_out` green; mono keeps outline/cell/ports pairwise distinct, terminal all-reset) — and the label overlay hint, which reuses `modifier_hue(hash%16)` (same hue family as the influence wash) with `graph_edge_error` red precedence kept.

Three built-in themes ship, selected by name (case-insensitive; `-`, `_`, and space are interchangeable separators):

| Theme | Character |
|---|---|
| `classic` | The original ANSI palette: kind colors white/magenta/cyan/green/red, shifts yellow/cyan/magenta/green, blue accents, dark-gray chrome |
| `terminal` | Every token is `Color::Reset`, so each user's terminal scheme supplies all colors — works with custom schemes and low-color terminals |
| `mono` | Grayscale only; shift tokens are pairwise distinct because shift groups are told apart by color alone during normal patching |

The choice persists in `$XDG_CONFIG_HOME/droid-tui/config.toml` as `theme = "…"` plus `[labels] layers_enabled = true` and `max_shift_layer = 4` (clamped 1..8, disabled coerces display to layer 1 while preserving 2..N). A missing file silently selects `classic` + label defaults; a malformed file or unknown theme name warns once on stderr at startup and falls back to `classic` (and clamped `[labels]`). An optional `[latency] per_circuit` map (lowercased circuit name → AVG microseconds) overrides the ramsize-proportional per-circuit cost model that drives the latency ramp and the optimizer. Per-patch labels live in `$XDG_CONFIG_HOME/droid-tui/labels.toml` keyed by canonicalized absolute patch path (`hw` per-token per-shift + `circuits` per-`NodeId`). The theme is installed before the terminal UI initializes, so a session never renders with a half-selected palette.

## Component Anatomy

Each component occupies a fixed cell of 16 columns × 3 rows. A scale factor is tracked through fixed presets of 75 %, 100 %, 150 % and 200 % (`+`/`-`, wrapping around at both ends) and reported in the status bar as `Scaling: N%`; the 75 % floor keeps module cells at a boxable width. It does not currently resize the rendered cells — components always render at 16×3 and the published hit rects match that fixed size.

Components without a parse-time LED association render as two-line text cells:

- **Row 1**: a state glyph followed by the component label (e.g. `● TRIG A`).
- **Row 2**: the state text (ON/OFF, percentage, CV IN/CV OUT), rendered in muted gray.

**Boxed cells for LED-associated components.** When a component's `.ini` section declares an LED association — a bare `led = L.N` entry, or a numbered circuit param `ledN = L.M` (e.g. `led11 = L1.1`) that shares its numeric suffix with a same-suffix element entry (`button11 = B1.1`, `pot11 = P1.1`, `encoder11 = E1.1`, `switch11 = S1.1`, `fader11 = M1.1`) in the same section, the DROID convention for circuits like `matrixmixer` — the association is parsed into `HwComponent.led` and the component renders as one bordered box filling its full cell instead of a bare text cell. The boxed path covers every control kind that can carry a resolvable LED association: each kind renders its own state inside the box (button/switch ON/OFF glyph, knob/encoder/fader percentage). At cell widths narrower than the box content, the content shrinks to fit inside a complete box or the cell falls back to unboxed two-line rendering — partial border fragments never appear:

- The box border uses the owning component's kind color — button/switch white, knob/encoder magenta, CV in cyan, CV out green, LED red.
- The element symbol + label live in the box's top title row, drawn inside the border row; the single interior row holds the element state text plus the LED glyph (`◉` lit, `○` unlit) reflecting the associated LED component's live state — one state, not a second textual LED state.
- Hover applies the same reversed/dark-gray emphasis to box content and border that text cells use.
- Components whose LED id does not resolve to an existing LED component fall back to the unlit glyph/state.
- LEDs referenced this way are never rendered as standalone grid cells; only unreferenced LEDs appear on their own.
- Over-long labels truncate with a trailing ellipsis (`…`) while cell geometry and hit rects stay unchanged.

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
- Same-kind component rows keep a uniform vertical rhythm; boxed (height 3) and unboxed (height 2) cells do not create irregular gaps between rows.
- With a shift active: panels containing the active shift group get a bold border in the group color and a `[SHIFT n]` title marker; all other panels dim to dark gray.
- With a modifier active (e.g. `B1.1`→`_TRIG`): influenced cells (boxed LED-cells and text cells inside modules) render with a background wash in `hash(token)%16` modifier hue; unaffected cells dim slightly. Rendering priority is `graph_edge_error` (red) > modifier hue > `CableKind`. Modifier background wash is orthogonal to shift borders — both can coexist (yellow shift border + modifier hue cell bg, e.g. `SHIFT 1` + `B1.1`). Interaction: `Mouse Down` without mods = momentary preview (cleared on `Up`/`Leave`), `Ctrl+Shift+Click` (alias `Ctrl+Click`) = toggle latched, `m` = keyboard alias for hovered component, `Esc` clears shift + modifier; single-var today, additive `MOD B1.1+B1.2 → N cells / M cables` is aspirational.
- With labels: HW panel cells show `display_label(token, shift)` (`store[layer]→store[1]→preamble[1]→derived`, `effective_shift` clamped 1..8, `layers_enabled=false` coerces to layer 1); source section headers and graph node titles show `circuit_label` override in both FULL and FILTERED panes when present. The centered single-field edit overlay (1-line input + hint) reuses the same `modifier_hue` for its hint/status `B3.17 / Group<N> → N ckts / M cables` (shift-blind structural `influence_subtree`, `graph_edge_error` red > modifier hue), responsive per width (`graph_edge_error` red precedence kept); `e` enters, `1..N` cycles Group layer preserving per-layer drafts, `Enter` saves (atomic `labels.toml` rewrite), `Esc` cancels.

## Physical View

- The main view is a **physical 1:1 layout**: a millimeter-accurate grid model of the rack (case rows, fold bars, mount sections, module faceplates) mapped to screen cells with aspect-compensated factors (columns/mm ≠ rows/mm so physical proportions survive ~2:1 terminal cells).
- **Skeleton reference mode**: `s` swaps the full render for a pure geometry outline — case border, fold-bar dividers labeled with their row, mount regions, module frames, and element-cell markers (`·` elements, `◀`/`▶` CV in/out ports) — in the `physical_skeleton_*` tokens. It is a presentation of the same layout, not a separate surface.
- **Zoom**: `+`/`-` cycle the presets 75 %, 100 %, 150 %, 200 % with wrap-around (floor 75 % keeps cells boxable); the status bar reports `Scaling: X%` (or the rack-aware physical hint). Zoom actually rescales the physical cells, unlike the legacy fixed 16×3 wrapped-panel cells.
- **Pan**: arrow keys pan the rack toward the pressed direction when it overflows the main area, and Up/Down fall back to panel navigation when it fits; `j`/`k` always navigate. The mouse wheel pans on overflow — a wheel over a knob/fader cell still adjusts its value when no overflow forces panning.
- **Rack definition**: `config.toml` gains `[physical]` view defaults (`show_skeleton`, `zoom`, `offset_x`/`offset_y`) and `[physical.rack]` (`rows = [{he, hp, label?}]`, `top_mount_te`, `side_mount_te`, `assign = { "P2B8 1" = 1 }`); absent sections keep the out-of-box single-row case wide enough for the whole chain.
- **Element state rendering**: each element renders its live state on its physical-view cell — buttons/switches show their toggle glyph, knobs/encoders/faders their percentage, CV I/O their direction — mirroring the panel view's state rendering.
- **Border abutment + switch placement**: adjacent module borders abut exactly at every zoom preset (edge-rounded mm→screen spans share boundary values); switch cells place per the controller's geometry data and never collapse onto a neighboring control's cell (e.g. a knob's) when geometry lacks a matching switch cell.

## Status Bar

A dark-gray band at the bottom, bordered, left-aligned. It shows the current status message in white, appends ` | SHIFT n ACTIVE` in the group color, bold, when a shift is active, appends ` | MOD B1.1 → N cells / M cables` in the modifier hue (bold) when a modifier is active (both can coexist), appends ` | Renders degraded at N cols — use ≥ M cols or reduce scale` in the `render_outlier_warning` token when a patch load is predicted to render degraded at the current terminal width/theme, and permanently displays the current display settings as `Scale: <factor> | Orientation: <Portrait|Landscape>`. Mouse `Down` = momentary modifier preview, `Ctrl+Shift+Click` (or `Ctrl+Click`) = toggle latched, `m` = keyboard alias, `Esc` clears shift + modifier.

## File Picker

An overlay centered in the terminal, roughly 70% of the width and 50% of the height, with a blue-bordered block titled ` File Picker `. Entries are listed with a `▶` marker on the selected row. The picker is a functional browser: directories and `.ini` files are selectable, other files are not. When not at the filesystem root, the parent-directory entry is the first entry, rendered as `..`, and Enter on it navigates up without closing the picker; at the root no `..` entry appears. Entries sort directories first, then `.ini` files.

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
- **Edges**: cables render as box-drawing polylines (`│ ─ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`) drawn cell by cell between the ports, with the port cells covered by the node frames for a clean join. An edge's color is the cable kind of the producing circuit (its declared `cable_kind` when a plugin circuit declares one, else name-substring inference) — **control** (circuits named clock/gate/trigger/pulsar/div) cyan, **audio** (the default) green, **midi** (midi/note/seq/pitch) magenta, and **unknown** dark gray when no edge produces the cable — overridden by the red `graph_edge_error` token when a topology-validation finding (dangling sink, `n → 1`, or a rack-wiring outlier) references the cable. When the structural diff is shown the colors shift again: added or changed cables draw in the `graph_edge_diff_added` green and removed cables in the `graph_edge_diff_removed` magenta (error red still wins over diff). Cable latency coloring (on by default, `c` toggles on the surface) replaces the kind color for non-error, non-diff cables with a blue→red ramp of five stops: `ramp[round(L/(N×AVG) × (stops−1))]`, where `L` is the edge's forward-loop latency in loop units and `AVG` the per-circuit ramsize-proportional mean — back-edge cables (source after sink) always land on the hottest stop. The status bar appends `latency avg X / max Y (1 loop ≈ 190µs) | N back edge(s)` and hovering a back-edge sink shows `reads _X 1 loop behind`. Edges incident to a circuit instance with processing disabled render with the `graph_edge_dim` token (`dim` modifier), overriding the cable-kind, diff, and modifier-hue colors but preserving the red error highlight.
- **Wiring-outlier detection**: a learned decision table (`geometry::WiringOutlierScorer`, embedded artifact `tools/outlier_artifact.txt` fitted by `tools/fit_outlier_model.py` on `corpus/features.csv`) classifies direct hardware bindings (zero cable hops) as implausible from `BindingFeatures` — euclidean/manhattan distance, controller/rack flags, source/sink kind — with the invariant guards (adjacent, co-located `L→B`, via-cable) applied at the call site before the scorer and a preserved threshold fallback (`euclidean > 8.0 && cable_hops == 0`) on a table miss (designs D1/D5). A second opinion (`patch::InfluenceStats`, embedded `tools/influence_stats.txt`) z-scores each hardware token's `influence_subtree` size against per-kind corpus mean/std and flags tokens beyond the 3.0 band. Each finding is a `Warning` topology finding carrying a synthetic cable name (`A->B`, or the token's first root var for the influence opinion) so it lights the affected edge in red via the reuse of `graph_edge_error`. This is validation/hygiene only — findings never block building or viewing the graph, and neither channel gates patch loading.
- **Nodes**: ComfyUI-style rounded frames (`BorderType::Rounded`), 22×5 cells, titled with the circuit name (or `circuit_label` override when a per-`NodeId` label exists — applies in both FULL and FILTERED panes); repeated names append the instance index (`copy`, `copy (1)`, `copy (2)`). The node border is white and the title yellow. A left `◉` input port marks nodes that consume cables; a right `●` output port marks producers (presence markers, not per-parameter pairing). A node whose circuit instance has processing disabled renders with the `graph_node_dim` token (`dim` modifier), overriding the modifier hue; hover styling stays visible on dimmed nodes.
- **Interaction**: while the surface is open it owns all mouse input. Left-dragging a node repositions it; on release the layout locally re-settles around the node (damped, bounded iteration budget) while distant nodes stay anchored. `x` toggles processing for the hovered circuit instance — the graph rebuilds, influence recomputes (dead-ending at disabled sinks), and the status bar reports `Processing disabled/enabled: <name> <instance>`; with no node hovered it is a silent no-op. `p` toggles the global processing pause (the same key works on the panel view). `Esc` closes the surface and restores the prior view; `q`/Ctrl+C still quit and `l` still opens the picker. The status bar continues to show the scale/orientation/shift state.

## Patch Diff

Opened with `g` then `d`, the patch diff loads a second patch (the B patch) through the file picker and highlights the structural difference from the currently-loaded patch (A). It is a read-only overlay on top of the graph and source surfaces — it never mutates the patch.

- **Trigger**: `g d` opens the picker in diff mode; picking a B patch computes `diff_patches(A, B)` (added/removed/changed cables, added/removed/changed circuit nodes) and emits `Event::DiffComputed`. `d` toggles the overlay on/off; `Esc` clears a component-scoped filter first, then hides the overlay.
- **Graph surface**: with the diff shown, edges whose cable was **added** or **changed** between A and B render in the `graph_edge_diff_added` green and **removed** cables in the `graph_edge_diff_removed` magenta. Topology-error red still takes precedence over the diff colors, and the diff colors take precedence over the cable-kind inference. A scoped diff (`diff_scope`, e.g. set to a single component token) filters the report so the graph highlights and the status hint's cable count match.
- **Nodes**: added or removed circuit nodes are distinguished on the surface alongside the edge highlights.
- **Status**: a trailing hint reports the diff scope and cable count in the status bar.

## Patch Validation

Loading a patch runs a schema-driven validation pass (`schema.rs` loads the vendored DROID circuit schema; `validation.rs` compares the parsed patch against it). Findings are sorted by (line, column) and each carries a severity — **error** (blocks clean patching), **warning**, or **hint** — plus a diagnostic code and message with its source span.

- **Modal**: when the load produces errors or warnings, a centered modal opens (≈60% width × 70% height, clamped, rounded border in `validation_modal_border`, plain-titled ` Validation (N) xE yW zH `). Each row reads `L{line}:{col} [E|W|H] [code] message` — the severity bracket in `validation_error` red / `validation_warning` yellow / `validation_hint` cyan (bold), the location in `text`, the code in `muted`, the message in `text`; the selected row is highlighted via `validation_selected_bg` + bold and non-selected rows dim. A fixed bottom hint lists the keys: `e` toggle, `j/k` navigate, `Enter` jump, `Esc` close.
- **Responsive**: on narrow terminals the modal shrinks to near full-main-width, mirroring the picker/overlay responsiveness.
- **Scope**: validation findings never block building or viewing the graph; they are informational feedback shown on load (the same `graph_edge_error` red lights offending cables in the graph).

## Circuit Plugins

Users can extend the circuit schema without rebuilding by dropping TOML plugin files into `$XDG_CONFIG_HOME/droid-tui/plugins/` (or a `[plugins] dir` override in `config.toml`; `[plugins] enabled = false` disables loading). Each plugin file contributes one or more `[[circuit]]` tables that merge **over** the embedded schema — a plugin circuit wins on a name collision (warn once per file), and a file that is malformed or missing the required `ramsize` is skipped with one warning (never aborts startup).

- **Format**: a circuit table declares `name` (case-insensitive, matching the embedded key convention), `category`, the required `ramsize` (bytes), optional `title`/`description`, optional rendering metadata `cable_kind` / `color`, and `inputs`/`outputs` parameter arrays (`#[serde(default)]` on both). Each parameter carries `name`, `short`, `type`, optional `default`, and the `prefix`/`count`/`start_at` expansion trio, so numbered plugin params expand through the same `expand_names` path as embedded ones.
- **`cable_kind`**: optional enum `control` / `audio` / `midi` / `unknown` (case-insensitive). When declared, it overrides the name-substring inference for edges produced by that circuit; when absent, inference applies exactly as before.
- **`color`**: optional component-kind theme token name (a resident ThemeColor key). When declared, the graph node for that circuit uses it; when absent, name-inference applies.
- **Merge rule**: `schema::merge_plugins(base, files)` applies plugin files in sorted-filename order (deterministic), insert-or-override on collision, warn-once shadow per file. Neutral defaults (`presets`, `manual`, per-param `essential`/`ramhint`/`autotitle`) are applied by the schema layer, not the plugin loader.
- **Rendering precedence**: declared metadata feeds the same precedence chain as before — `graph_edge_error` red > diff colors > modifier hue > `CableKind` (now declared-or-inferred) > dim for disabled instances. Plugin circuits participate in `ram_overflow` validation and the latency cost model via their declared `ramsize` (never the unknown-circuit fallback), so a plugin circuit can never silently disable RAM validation.

## Optimizer

Opened with `g` then `o`, the optimizer menu proposes reorderings of the patch's `[section]` blocks that reduce forward-loop latency — the extra loop units a consumer circuit reads from a producer that runs later in the scan. Each candidate is a section permutation scored by a `CostModel` whose per-circuit average latency (`AVG`) is proportional to the circuit's RAM size (overridable per circuit via `[latency] per_circuit` in config). The menu is a centered modal overlay styled after the validation modal: border in `optimizer_modal_border`, selected row highlighted via `optimizer_selected_bg` + bold, non-selected rows dimmed; it renders above the validation modal and below the label-edit overlay.

- **Candidates**: up to three strategies are generated — banner min-sum (permutes within each banner group; degenerates to the whole file when the patch has no banners), global min-sum, and min-max (minimizes the worst forward-loop latency) — each labeled and shown with its `before → after` average/maximum latency summary. The identity ordering is always among the candidates.
- **Keys**: `j`/`k` (or Up/Down) move the selection, `Enter` previews the selected candidate, `r` restores the original file order, `s` exports the selected candidate, `Esc` closes the menu (restoring the file order when a preview is active).
- **Preview**: applying a candidate reorders `patch.sections` in place and rebuilds the graph, so the latency ramp recolors live and the status bar reads `Preview: <label>`. The original order is remembered and restored by `r`, `Esc`, or previewing another candidate.
- **Export**: `s` writes a reordered copy of the patch to `<stem>-latopt.ini` next to the source file via the lossless writer (byte-identical round-trip, atomic write, auto-suffix on collision); the status bar reports `Exported <label> → <path>`. The loaded patch is never mutated.

## Processing Pause

Pressing `p` toggles `App.processing_paused`, a global pause of the simulated processing. While paused, all panel content renders with the `dim` modifier (borders and cells), the header shows a `PROCESSING PAUSED` marker, and component mutations are blocked (toggles and value changes are no-ops until `p` is pressed again). Geometry (`component_rects`) is unchanged while paused, so mouse hit-testing keeps working. Pause state resets on patch load.

## Empty State

With no patch loaded, the main area shows the centered muted prompt `Press 'l' to load a patch`.

## Visual Validation Provenance

Face correctness is proven via the `insta` snapshot harness in `src/regression.rs` (`buffer_to_ansi` trims trailing empty cells, `buffer_to_html` maps fg/bg/bold/dim/reversed per span). The browsable gallery at `evidence/gallery/index.html` renders one row per scenario — fixtures `arpeggio1.ini`, `led_pairs.ini`, `source_navigation.ini`, `multi_module_p2b8.ini`, `numbered_led_pairs.ini` × themes `classic`/`terminal`/`mono` × widths 80/120 and viewer open/closed + shift1 — as HTML + ANSI sidecars, plus the `switch_value`/`paused_dim`/`disabled_circuit_graph` scenarios from the circuit-processing change, plus the overlay/label scenarios from the label-management change. Graph-surface faces are covered by the same snapshot harness (scenarios `cable_banner_combos.ini`, `graph_edge_kinds.ini`, `graph_topology_error.ini` × `classic`/`mono` × widths 40/100), asserting cluster/node frames, edge-kind colors, the topology-error highlight, and the disabled-node/edge dim. The patch-validation change adds a validation matrix across `fixtures/validation/*.ini` (duplicate/undefined/unused cables, duplicate/unknown/missing params, unknown circuits, invalid jacks, RAM overflow × `classic`/`terminal`/`mono`), and the diff and rack-wiring changes are covered by graph highlighting tests that assert the diff-added/diff-removed and wiring-outlier red edge colors appear only when the corresponding feature is active. The nn-ui-outlier-detection change adds a proof layer: a regression test asserts the fitted table beats the 8.0 rule on the holdout (0.824/1.000 vs 0.124/0.714), an invariant matrix proves the D5 guards and the fallback, and the `influence_outlier.ini` fixture snapshot renders both the wiring-outlier and the per-token influence-z-score channels on the graph surface. The latency ramp is pinned by a four-fixture snapshot matrix (`graph_latency_chain`, `graph_latency_fanout`, `graph_latency_backedge`, `graph_latency_error` × `classic`/`terminal`/`mono` × widths 100/40), and the optimizer menu plus its live preview-recolor are covered by `g o`-driven snapshots on `optimizer_latency.ini` (task 3.2). Output is ephemeral in the worktree (`.gitignore`'d, generated via `cargo run --bin snapshot-gallery` or `cargo test -- --generate-gallery`) and durable in the OpenSpec archive (`scripts/archive-gallery.sh` mirrors into `openspec/changes/archive/2026-08-24-add-visual-validation/evidence/gallery`); the strict gate (`cargo test` / `cargo insta test --check`) fails on any face mismatch. This is the `visual-validation` change; see `openspec/specs/visual-validation/spec.md`. The physical-scale-model change adds the physical view to the matrix — skeleton and full presentations render as separate rows for the physical fixtures (`led_pairs.ini`, `switch_value.ini`, `physical_multirow_rack.ini`) — and asserts the coincidence invariant (5.1): every skeleton element-cell rect equals the corresponding full-render rect for every token, across fixtures, themes, and viewports.

<!-- Last updated: 2026-08-31 · circuit-plugin-system: user TOML plugin files in $XDG_CONFIG_HOME/droid-tui/plugins/ (or [plugins] dir; [plugins] enabled=false disables) with [[circuit]] tables (name/category/ramsize + optional cable_kind/color + inputs/outputs with prefix/count/start_at) merged over the embedded schema (plugin wins on collision, warn-once shadow, malformed/missing-ramsize skipped) -->
<!-- Last updated: 2026-08-30 · physical-scale-model: physical 1:1 mm view (grid model, `s` skeleton reference mode, `Scaling: X%` zoom presets, arrow/wheel pan, [physical]/[physical.rack] config) + physical_skeleton_* tokens + skeleton|full gallery rows and coincidence assertion -->
<!-- Last updated: 2026-08-28 · latency-optimized-patch-generation: CostModel (AVG ∝ ramsize, [latency] per_circuit overrides) + forward-loop latency ramp (graph_edge_latency_0–_4, c toggle) + g o optimizer menu (optimizer_modal_border/optimizer_selected_bg; preview/restore/export to <stem>-latopt.ini) -->
