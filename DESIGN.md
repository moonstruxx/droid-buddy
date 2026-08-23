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
typography:
  family: terminal-default
  size: terminal-default
  modifiers:
    emphasis: bold
    de-emphasis: dim
    hover: reversed
spacing:
  component-width: 16
  component-height: 2
  header-height: 3
  status-height: 3
  main-min-height: 10
  panel-padding: 1
  picker-width-ratio: 0.7
  picker-height-ratio: 0.5
  viewer-sidebar-width-ratio: 0.2
  viewer-sidebar-min-width: 20
  viewer-status-height: 3
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
- **Interaction is forgiving.** Hover reverses the component's colors; click toggles; scroll adjusts values in small steps (±0.05). Keyboard and mouse are interchangeable — the same component is targeted whether reached by `j`/`k` navigation or by pointing.

## Component Anatomy

Each component occupies a fixed cell of 16 columns × 2 rows; a uniform scale factor multiplies every component's rendered width and height so the whole panel zooms together. The factor steps through fixed presets of 50 %, 100 %, 150 % and 200 % (`+`/`-`, wrapping around at both ends), and the status bar confirms each step as `Scaling: N%`.

- **Row 1**: a state glyph followed by the component label (e.g. `● TRIG A`).
- **Row 2**: the state text (ON/OFF, percentage, CV IN/CV OUT), rendered in muted gray.

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
- Components are first grouped into module containers (one per circuit), then modules flow left-to-right within a row and wrap to additional rows when the terminal is too narrow; each row is exactly one component-height.
- Panel layout follows the display orientation: panels stack vertically in portrait and arrange horizontally in landscape.
- Panel borders are dark gray by default.
- With a shift active: panels containing the active shift group get a bold border in the group color and a `[SHIFT n]` title marker; all other panels dim to dark gray.

## Status Bar

A dark-gray band at the bottom, bordered, left-aligned. It shows the current status message in white, appends ` | SHIFT n ACTIVE` in the group color, bold, when a shift is active, and permanently displays the current display settings as `Scale: <factor> | Orientation: <Portrait|Landscape>`.

## File Picker

An overlay centered in the terminal, roughly 70% of the width and 50% of the height, with a blue-bordered block titled ` File Picker `. Entries are listed with a `▶` marker on the selected row. The picker is a functional browser: directories and `.ini` files are selectable, other files are not.

## Source Viewer

Opened with `g` then `v`, the source viewer is embedded in the existing three-band TUI. The header and bottom status band remain; the flexible main band becomes a 50/50 split between the hardware panels and the source viewer. An open file picker still has absolute precedence and renders over the viewer.

- **Panels pane** (left half): a bordered ` Panels ` block containing the normal hardware layout. Its border is bold yellow when panel focus is active; otherwise it is dark gray. Panel interactions are isolated while source focus is active.
- **Source pane** (right half): internally split into a circuit sidebar, scrolling source content, and an optional minimap. The sidebar is ~1/5 of the source-pane width, with a 20-column minimum while retaining at least 20 columns for content. It is a blue-bordered ` Circuits ` block listing every `[section]` in parse order; repeated names are disambiguated as `copy`, `copy (1)`, `copy (2)`. The selected entry uses reversed video; other entries are white.
- **Focus emphasis**: the source content border and title are bold yellow while source focus is active, and dark gray while panel focus is active. `Tab` switches focus; `Esc` closes the viewer while preserving selection and source position.
- **Raw mode** (default): the content pane shows retained verbatim `.ini` lines, including comments and blank lines, with vertical scroll. The title is ` Source [raw] `. `t` toggles to prettified mode without closing the viewer.
- **Prettified mode**: each circuit is rendered as a small ASCII box — a `┌─ name ─┐` cap with a bold, circuit-colored name, one `│ key = value │` line per setting with cyan keys and white values, a `└────┘` base, and a blank line. Circuit frame colors reuse the component palette: buttons/switches/notebuttons white, pots/encoders/faderbank magenta, CV in cyan, CV out green, LEDs red, and unknown circuits blue.
- **Selection highlights**: selected-token occurrences are yellow and bold; the current occurrence is yellow, bold, and reversed on dark gray. Affected boolean `select`/transitive modifier spans are cyan, bold, and underlined. Affected exact-value (`selectat`) spans are magenta, bold, and underlined. In prettified mode, affected values use the same cyan/magenta modifier colors; token references are yellow, bold, and reversed. Clearing selection clears these highlights.
- **Minimap**: when the loaded patch and terminal are wide enough, a ` Map ` column is 3 columns wide (plus borders) and summarizes the full raw file. Plain lines use dark-gray `·`; occurrence lines use yellow `█`; modifier lines use cyan `▓` for boolean or magenta `▓` for exact-value relationships (combined occurrence/modifier lines are magenta `█`). The visible source viewport is shown as a reversed dark-gray indicator and moves proportionally with source scroll. The renderer publishes the minimap rectangle for click-to-scroll hit testing. It is hidden when total width is below 80 columns, the source pane is below 60 columns, height is below 10 rows, or keeping it would reduce readable source content below 20 columns.
- **Viewer status bar** (bottom, 3 rows): a dark-gray bordered band reads bold-white `Source Viewer | ` followed by cyan shortcut tokens — `ESC` close, `j/k` scroll, `Up/Down` occurrence navigation, `Home/End` first/last occurrence, `t` mode toggle, and `Tab` focus.
- **Empty states**: centered muted `No patch loaded` or `No circuits in patch` appears inside the source content border; the sidebar remains an empty bordered block.
- **Readonly and isolation**: the source pane never toggles components. While source focus is active, panel toggles, shift selection, scale, and orientation keys are inert; `Tab` returns panel focus. Panel interaction and shift visualization remain available from the panel-focused state.

## Empty State

With no patch loaded, the main area shows the centered muted prompt `Press 'l' to load a patch`.

<!-- Last updated: 2026-08-23 -->
