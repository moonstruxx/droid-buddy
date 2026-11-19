# Spec Delta

## MODIFIED Requirements

### Requirement: Non-latching maximize

`z` and `f` SHALL toggle the focused pane to fill the whole main band and back to its arrangement. Maximize SHALL NOT latch: pressing `z` or `f` again, moving focus to another pane, or pressing `Esc` SHALL restore the arrangement. While a pane is maximized the other panes SHALL not render.

#### Scenario: Maximize the focused pane

- **WHEN** a small pane is focused and the user presses `z`
- **THEN** that pane fills the main band and the other panes are hidden

#### Scenario: f is the second maximize binding

- **WHEN** any pane is focused and the user presses `f`
- **THEN** that pane fills the main band, and pressing `f` again restores the arrangement

#### Scenario: Restore the arrangement

- **WHEN** a pane is maximized and the user presses `z` again
- **THEN** the arrangement returns with the same views in the same panes

#### Scenario: Esc restores the arrangement

- **WHEN** a pane is maximized and the user presses `Esc`
- **THEN** the maximize clears and the arrangement returns

#### Scenario: Focus change restores the arrangement

- **WHEN** a pane is maximized and the user presses `Tab`
- **THEN** the maximize clears, focus moves to the next pane, and the arrangement returns
