# Viewer Layout Specification

## Purpose

Embedded source-pane layout for displaying DROID `.ini` patch source in the main TUI. The viewer renders as a split pane beside the hardware panels; a left sidebar lists circuits as jump points; the main area renders either verbatim raw text or prettified circuit blocks with styled key-value pairs, plus a full-file minimap.

## Requirements

### Requirement: Embedded source pane layout
The source viewer SHALL render as a slot in the right column of the tiled layout, not as a fixed split pane. When the source viewer is the only view in the right column, it shares the width split ratio with the left panel pane (default 60% panels / 40% source). When multiple views share the right column, the source viewer occupies one horizontal slot and its height is determined by the number of visible slots. The source pane's internal two-pane layout (sidebar + main area) remains unchanged.

#### Scenario: Default split with source only
- **WHEN** the user opens the source viewer as the only view in the right column
- **THEN** the panels column takes 60% of the width and the source pane takes 40%

#### Scenario: Source with other views
- **WHEN** the source viewer shares the right column with the graph view
- **THEN** the source pane occupies one horizontal slot in the right column at the configured split ratio

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

### Requirement: Empty patch state
When no patch is loaded, the source main area SHALL display "No patch loaded" centered in dark-gray and the sidebar SHALL render empty with only its border.

#### Scenario: Viewer opens without patch
- **WHEN** the source pane opens with `viewer_patch` empty
- **THEN** the main area shows "No patch loaded" and the sidebar is empty inside its border

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
Exactly one pane SHALL be focused at a time; the focused pane SHALL be visually emphasized via the `pane_focus_border` theme token. When the source pane is focused, its border renders with the focus token; when focus moves away, the border returns to `pane_unfocused_border`.

#### Scenario: Focus follows Tab
- **WHEN** the source pane is focused and the user presses `Tab`
- **THEN** focus moves to the next pane in the carousel and the source pane's border updates to unfocused styling

## Design Decisions

- Decision 1: Sidebar width is computed as `max(20, width / 5)` capped to leave at least 20 columns for the main area. Rationale: ensures readability on narrow terminals while giving the sidebar enough space for circuit names.
- Decision 2: Circuit type colors map to ANSI 16 palette matching the existing component-kind color system (button/switch→white, pot/encoder→magenta, cvin→cyan, cvout→green, led→red, default→blue). Rationale: consistent visual language between the main view and the source viewer.
- Decision 3: Box-drawing characters (┌─ ─┐ │ │ └────┘) create circuit block borders rather than ratatui `Block` widgets. Rationale: allows multiple circuit blocks to flow as continuous text within a single scrollable `Paragraph`, avoiding per-block layout splitting.
- Decision 4: Disambiguation uses a simple first-seen counter rather than a global index. Rationale: the first occurrence of a name stays clean; only duplicates get suffixes, matching user expectation from file-manager conventions.
- Decision 5: Minimap geometry is published alongside `component_rects` by the renderer each frame. Rationale: the renderer owns layout, so only it knows where the minimap landed; the handler consumes that geometry for click-to-scroll hit-testing.
