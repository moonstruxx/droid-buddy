# keybinding Specification (delta)

## ADDED Requirements

### Split ratio keys

When the source viewer is open, the `[` and `]` keys SHALL adjust the panels|source split ratio.

**Scenarios:**

- **Widen source**: Pressing `]` increases the source pane width by 10% (panels shrink).
- **Narrow source**: Pressing `[` decreases the source pane width by 10% (panels grow).
- **Ratio clamped**: The ratio cannot go below 30% or above 70% for either side.
- **Viewer closed**: `[` and `]` have no effect when the viewer is not open.
