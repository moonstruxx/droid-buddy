# Spec Delta

## ADDED Requirements

### Requirement: Module UI camera keys

On the module UI pane, `c` SHALL centre the view on the focused element at the current zoom, and SHALL centre the rack when no element is focused. `Shift+C` SHALL apply the labels fit and SHALL clear the manual-navigation latch, so the automatic fit triggers — view open, patch load, rack change, pane resize — fit the view again. Both keys SHALL be silent no-ops on every other pane, and the help table for the pane SHALL list both, alongside the minimap's click and drag gestures.

The pair SHALL mean on the module UI what it already means on the signal-flow graph: bare `c` centres, `Shift+C` refits.

#### Scenario: c centres the focused element

- **WHEN** an element is focused on the module UI and the view has been panned away from it, then `c` is pressed
- **THEN** the pane pans so that element sits at the pane's centre, and the zoom does not change

#### Scenario: c with nothing focused centres the rack

- **WHEN** no element is focused and `c` is pressed
- **THEN** the rack is centred in the pane at the current zoom

#### Scenario: Shift+C re-arms automatic fitting

- **WHEN** the user has panned or zoomed by hand, which latches the view against automatic fits, and then presses `Shift+C`
- **THEN** the labels fit is applied immediately and a later patch load or pane resize fits the view again without another keypress

#### Scenario: The keys do not leak to other panes

- **WHEN** `c` or `Shift+C` is pressed while the source viewer, the graph, the picker or the optimizer holds focus
- **THEN** the module UI's camera does not move and the focused pane keeps its own meaning for those keys

#### Scenario: The help table documents the pair

- **WHEN** the help modal is opened with the module UI focused
- **THEN** it lists `c` as centring the focused element and `Shift+C` as fitting and re-arming, and it lists the minimap's click and drag gestures
