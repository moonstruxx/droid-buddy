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

Each component occupies a fixed cell of 16 columns × 2 rows:

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
- Components flow left-to-right within a row and wrap to additional rows when the terminal is too narrow; each row is exactly one component-height.
- Panel borders are dark gray by default.
- With a shift active: panels containing the active shift group get a bold border in the group color and a `[SHIFT n]` title marker; all other panels dim to dark gray.

## Status Bar

A dark-gray band at the bottom, bordered, left-aligned. It shows the current status message in white, and when a shift is active appends ` | SHIFT n ACTIVE` in the group color, bold.

## File Picker

An overlay centered in the terminal, roughly 70% of the width and 50% of the height, with a blue-bordered block titled ` File Picker `. Entries are listed with a `▶` marker on the selected row. The picker is a functional browser: directories and `.ini` files are selectable, other files are not.

## Empty State

With no patch loaded, the main area shows the centered muted prompt `Press 'l' to load a patch`.

<!-- Last updated: 2026-08-20 -->