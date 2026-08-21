# Viewer Layout Specification


## Purpose

Two-pane diff-viewer layout for displaying DROID `.ini` patch circuits in the
source viewer. A left sidebar lists circuit names as jump points; the main area
renders prettified circuit blocks with styled key-value pairs.

## Requirements

- REQ-1: Left sidebar occupies ~20% of terminal width (minimum 20 columns, capped so main area retains at least 20 columns). Bordered with `Color::Blue`, titled "Circuits".
- REQ-2: Sidebar lists every circuit from `app.viewer_patch` in parse order. The circuit at `app.viewer_selected_circuit` is highlighted with `Modifier::REVERSED`.
- REQ-3: Repeated circuit names are disambiguated with a suffix index: the first occurrence is bare, subsequent ones get " (1)", " (2)", etc.
- REQ-4: Main area occupies remaining width. Bordered with `Color::DarkGray`.
- REQ-5: Each circuit renders as a bordered block (chat-bubble style using box-drawing characters). Circuit name is the block title, styled bold and colored by circuit type.
- REQ-6: Key-value pairs render as lines inside the block: `key = value`. Keys use `Color::Cyan`, values use `Color::White`.
- REQ-7: Main area scrolls vertically via `Paragraph::scroll((app.viewer_scroll, 0))`.
- REQ-8: Status bar at bottom shows "Source Viewer | ESC to close | j/k scroll | Enter to jump" with dark-gray background.
- REQ-9: When `viewer_patch` is `None`, main area displays "No patch loaded" centered in dark-gray.

## Design Decisions

- Decision 1: Sidebar width is computed as `max(20, width / 5)` capped to leave at least 20 columns for the main area. Rationale: ensures readability on narrow terminals while giving the sidebar enough space for circuit names.
- Decision 2: Circuit type colors map to ANSI 16 palette matching the existing component-kind color system (button/switch→white, pot/encoder→magenta, cvin→cyan, cvout→green, led→red, default→blue). Rationale: consistent visual language between the main view and the source viewer.
- Decision 3: Box-drawing characters (┌─ ─┐ │ │ └────┘) create circuit block borders rather than ratatui `Block` widgets. Rationale: allows multiple circuit blocks to flow as continuous text within a single scrollable `Paragraph`, avoiding per-block layout splitting.
- Decision 4: Disambiguation uses a simple first-seen counter rather than a global index. Rationale: the first occurrence of a name stays clean; only duplicates get suffixes, matching user expectation from file-manager conventions.

## Scenarios

### Scenario: Viewer opens with loaded patch
Given `app.showing_viewer` is true
And `app.viewer_patch` contains circuits
When the viewer renders
Then the sidebar shows all circuit names with the selected one reversed
And the main area shows styled circuit blocks scrollable with j/k

### Scenario: Viewer opens without patch
Given `app.showing_viewer` is true
And `app.viewer_patch` is `None`
When the viewer renders
Then the main area shows "No patch loaded"
And the sidebar renders empty with only its border

### Scenario: Repeated circuit names
Given a patch with three circuits named "copy"
When the sidebar renders
Then the entries show "copy", "copy (1)", "copy (2)"

### Scenario: Narrow terminal
Given terminal width is 50 columns
When the viewer renders
Then the sidebar gets 20 columns (minimum)
And the main area gets 30 columns
