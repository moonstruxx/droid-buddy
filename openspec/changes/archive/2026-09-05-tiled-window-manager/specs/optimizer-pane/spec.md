## Purpose

The latency optimizer as a side pane instead of a centered modal, allowing the main panel view to remain visible while comparing candidates and previewing reorderings.

## ADDED Requirements

### Requirement: Optimizer as side pane

The latency optimizer (`g o`) SHALL open as a pane in the right column of the tiled layout, not as a centered modal overlay. The optimizer pane lists candidate section orderings with before/after latency summaries. The main panel view remains visible in the left pane while the optimizer pane is open.

#### Scenario: Open optimizer pane

- **WHEN** a patch is loaded and the user presses `g o`
- **THEN** the optimizer opens as a pane in the right column, and the panel view remains visible in the left pane

#### Scenario: Candidates visible with panels

- **WHEN** the optimizer pane is open
- **THEN** candidate orderings render with `before → after` summaries, and the panel view shows the current patch state in the left pane

### Requirement: Optimizer pane interactions

While the optimizer pane has focus, `j`/`k` navigate candidates, `Enter` previews the selected candidate (reorders `patch.sections` in place and rebuilds the graph so the latency ramp recolors live), `r`/`Esc` restores the original order, `s` exports the selected candidate, and `[`/`]` adjust the weighted slider objective `w`. The status line shows the active candidate label while a preview is loaded.

#### Scenario: Preview recolors live

- **WHEN** a candidate is selected and the user presses `Enter`
- **THEN** the patch sections reorder, the graph rebuilds, and the latency ramp recolors; the panel view reflects the new order

#### Scenario: Esc restores original

- **WHEN** a candidate is previewed and the user presses `Esc`
- **THEN** the original section order restores and the optimizer pane closes

### Requirement: Optimizer replaces modal

The previous centered modal rendering (`render_optimizer_modal`) SHALL be removed. All optimizer state and rendering migrate to the side-pane path. The theme token `optimizer_modal_border` is replaced by the pane focus border tokens.

#### Scenario: No modal rendering

- **WHEN** the optimizer is open
- **THEN** no centered modal overlay renders; the optimizer occupies a pane slot in the right column
