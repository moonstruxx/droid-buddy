# Optimizer Pane Specification

## Purpose

The latency optimizer as a side pane instead of a centered modal, allowing the main panel view to remain visible while comparing candidates and previewing reorderings.

## Requirements

### Requirement: Optimizer as side pane

The latency optimizer (`g o`) SHALL open as a view in a small pane of the class-based pane layout, not as an overlay and not as a centered modal. The optimizer pane lists candidate section orderings with before and after latency summaries. The big pane keeps its current view while the optimizer pane is open.

#### Scenario: Open optimizer pane
- **WHEN** a patch is loaded and the user presses `g o`
- **THEN** the optimizer opens in a small pane and the big pane keeps its current view

#### Scenario: Candidates visible with panels
- **WHEN** the optimizer pane is open
- **THEN** candidate orderings render with before and after summaries while the module UI stays visible in the big pane

#### Scenario: No overlay card
- **WHEN** the optimizer is open
- **THEN** no overlay card is painted over any other pane and the optimizer occupies only its own pane

### Requirement: Optimizer pane interactions

While the optimizer pane has focus, `j`/`k` navigate candidates, `Enter` previews the selected candidate (reorders `patch.sections` in place and rebuilds the graph so the latency ramp recolors live), `r`/`Esc` restores the original order, `s` exports the selected candidate, and `[`/`]` adjust the weighted slider objective `w`. While the optimizer pane is focused, `[` and `]` SHALL adjust `w` and SHALL NOT move the pane boundaries. The status line shows the active candidate label while a preview is loaded.

#### Scenario: Preview recolors live
- **WHEN** a candidate is selected and the user presses `Enter`
- **THEN** the patch sections reorder, the graph rebuilds, and the latency ramp recolors

#### Scenario: Esc restores original
- **WHEN** a candidate is previewed and the user presses `Esc`
- **THEN** the original section order restores and the optimizer pane closes

#### Scenario: Weight key stays view-local
- **WHEN** the optimizer pane is focused and the user presses `]`
- **THEN** the objective weight increases and the big pane boundary does not move

### Requirement: Optimizer replaces modal

The previous centered modal rendering (`render_optimizer_modal`) SHALL be removed. All optimizer state and rendering migrate to the side-pane path. The theme token `optimizer_modal_border` is replaced by the pane focus border tokens.

#### Scenario: No modal rendering

- **WHEN** the optimizer is open
- **THEN** no centered modal overlay renders; the optimizer occupies a pane slot in the right column