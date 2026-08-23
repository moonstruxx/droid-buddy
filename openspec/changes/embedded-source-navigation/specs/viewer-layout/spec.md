# Viewer Layout Specification (delta)

## MODIFIED Requirements

### Requirement: Embedded source pane layout
The source viewer SHALL render as an embedded split pane of the main TUI instead of a separate process window: hardware panels on the left, source pane on the right, with the file-picker overlay taking precedence over the source pane when open.

#### Scenario: Embedded split replaces external window
- **WHEN** user presses `g` then `v`
- **THEN** the source pane appears inside the running TUI; no second process, herdr pane, or terminal window is spawned

#### Scenario: Picker precedence preserved
- **WHEN** the source pane is open and the user opens the file picker
- **THEN** the picker overlay renders above the source pane

### Requirement: Two-pane circuit layout
The source pane SHALL keep the diff-viewer style two-pane layout: a left sidebar listing circuits and a main area rendering circuit content. The sidebar occupies ~20% width (minimum 20 columns, capped so the main area retains at least 20 columns), bordered `Color::Blue` titled "Circuits"; the main area occupies the remaining width bordered `Color::DarkGray`.

#### Scenario: Sidebar and main area proportions
- **WHEN** the source pane renders in a 100-column terminal
- **THEN** the sidebar is 20 columns wide and the main area takes the remaining 80

#### Scenario: Narrow terminal
- **WHEN** the terminal is 50 columns wide
- **THEN** the sidebar gets 20 columns (minimum) and the main area gets 30

### Requirement: Sidebar circuit jump points
The sidebar SHALL list every circuit from the loaded patch in parse order. The selected sidebar entry SHALL be highlighted with `Modifier::REVERSED`. Repeated circuit names SHALL be disambiguated with a suffix index: the first occurrence bare, subsequent ones " (1)", " (2)", etc.

#### Scenario: Sidebar lists circuits in parse order
- **WHEN** a patch with multiple circuits is loaded
- **THEN** the sidebar lists them in parse order with the current selection reversed

#### Scenario: Repeated circuit names
- **WHEN** a patch has three circuits named "copy"
- **THEN** the sidebar shows "copy", "copy (1)", "copy (2)"

### Requirement: Prettified circuit rendering
In prettified view mode each circuit SHALL render as a bordered block (box-drawing chat-bubble style) flowing as continuous scrollable text: circuit name as bold block title colored by circuit type (button/switch→white, pot/encoder→magenta, cvin→cyan, cvout→green, led→red, default→blue); key-value pairs as `key = value` lines with cyan keys and white values.

#### Scenario: Circuit blocks styled by type
- **WHEN** prettified mode renders a `[button]` and a `[lfo]` circuit
- **THEN** the button block title is white and the lfo block title falls back to blue

### Requirement: Source area scrolling
The source main area SHALL scroll vertically through its content, saturating at the top and clamping at the bottom of the rendered content.

#### Scenario: Scroll bounds respected
- **WHEN** the user scrolls above the first line or below the last line
- **THEN** the scroll position saturates at the boundary instead of underflowing or overshooting

### Requirement: Viewer status bar
While the source pane is open, a status bar SHALL render at the bottom with dark-gray background showing the viewer hints including close (`ESC`), line scroll (`j/k`), occurrence navigation (`Up/Down`, `Home/End`), view-mode toggle (`t`), and focus switch (`Tab`).

#### Scenario: Status hints reflect controls
- **WHEN** the source pane is open
- **THEN** the status bar mentions ESC to close, j/k scroll, Up/Down occurrences, t for raw/prettified, and Tab focus

### Requirement: Empty patch state
When no patch is loaded, the source main area SHALL display "No patch loaded" centered in dark-gray and the sidebar SHALL render empty with only its border.

#### Scenario: Viewer opens without patch
- **WHEN** the source pane opens with `viewer_patch` empty
- **THEN** the main area shows "No patch loaded" and the sidebar is empty inside its border

## ADDED Requirements

### Requirement: Raw source view mode
The system SHALL provide a raw view mode showing the patch's verbatim `.ini` text with syntax-neutral styling, entered by default when the pane opens; `t` SHALL toggle between raw and prettified modes preserving the position of the content currently in view where a corresponding position exists.

#### Scenario: Raw mode default
- **WHEN** the source pane opens on a loaded patch
- **THEN** the main area shows the verbatim `.ini` text starting at the positioned line

#### Scenario: Toggle between modes
- **WHEN** the user presses `t`
- **THEN** the main area switches between raw text and prettified circuit blocks without closing the pane

### Requirement: Full-file minimap
The source pane SHALL render a minimap column summarizing the entire file so the visible viewport is indicated within the whole document; clicking a minimap position SHALL scroll the source to the corresponding line. On terminals too narrow for panels + sidebar + minimap + readable source, the minimap SHALL be hidden rather than squeezing the source below readability.

#### Scenario: Viewport indicator tracks scroll
- **WHEN** the source scrolls to the middle of the file
- **THEN** the minimap viewport indicator moves proportionally to the middle

#### Scenario: Minimap click scrolls source
- **WHEN** the user clicks the minimap at a position representing line N
- **THEN** the source scrolls so line N is in view and the indicator updates

#### Scenario: Minimap hidden on narrow terminals
- **WHEN** the terminal width cannot fit panels, sidebar, minimap, and minimum source width
- **THEN** the minimap is not rendered and the source keeps usable width

### Requirement: Pane focus indication
Exactly one of the panel area and the source pane SHALL be focused at a time; the focused side SHALL be visually emphasized (border color/intensity) so the target of keyboard input is unambiguous.

#### Scenario: Focus follows Tab
- **WHEN** the source pane is focused and the user presses `Tab`
- **THEN** focus moves to the panel area and the border emphasis switches accordingly
