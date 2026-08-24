# viewer-layout Specification (delta)

## MODIFIED Requirements

### Embedded source pane layout

The panels | source split ratio SHALL be adjustable and SHALL default in favor of the panels (60% panels / 40% source).

**Scenarios:**

- **Default split**: Opening the viewer shows panels at 60% width and source at 40%.
- **Adjust ratio**: Pressing `[` narrows the source pane; pressing `]` widens it. Each keypress adjusts by ±10%.
- **Ratio clamped**: The ratio is clamped to 30–70% — the source pane never takes more than 70% or less than 30% of the width.
