# Spec Delta

## MODIFIED Requirements

### Requirement: Physical-view navigation keys

The system SHALL provide keys to pan the physical view when the rack overflows the pane, to change zoom, to fit the rack to the pane, and to toggle the skeleton reference mode. Overflow SHALL be judged against the pane the module UI is actually drawn in, not against a stored viewport size, so panning works whenever the drawn rack exceeds the pane on the pressed axis.

#### Scenario: Pan keys move the viewport

- **WHEN** the rack is larger than the pane and the user presses a pan key
- **THEN** the viewport offset moves in the corresponding direction without changing zoom, and the status reports the offset

#### Scenario: Pan keys are inert on a fitting rack

- **WHEN** the drawn rack fits the pane on an axis and the user presses the pan key for that axis
- **THEN** the view does not move and the key falls through to its navigation meaning

#### Scenario: Fit key returns to the fitted view

- **WHEN** the user has zoomed and panned and then presses the fit key
- **THEN** the rack is scaled to fit the pane and centred, and the status reports the fit

#### Scenario: Skeleton toggle switches presentation

- **WHEN** the user presses the skeleton-toggle key
- **THEN** the main view switches between full and skeleton presentation of the same layout, and back.

### Requirement: Zoom family keys

The system SHALL consolidate zoom-like actions onto `+`/`-` with modifier variants:
- `+`/`-` (no modifier): scale the focused pane (the module UI steps its zoom presets when the module UI is focused, the graph camera zooms when graph is focused).
- `Shift++`/`Shift+-`: scale the other (non-focused) pane.
- `Alt+[`/`Alt+]`: adjust cable tension when graph has focus.

#### Scenario: Scale focused pane
- **WHEN** the graph pane is focused and the user presses `+`
- **THEN** the graph camera zooms in

#### Scenario: Scale other pane
- **WHEN** the graph pane is focused and the user presses `Shift++`
- **THEN** the panel pane scale increases

#### Scenario: Module UI zooms in presets

- **WHEN** the module UI pane is focused and the user presses `+`
- **THEN** the view steps to the next zoom preset above the current zoom, where the presets are multipliers of the true-1:1 baseline

#### Scenario: Module UI zoom wraps at the ends

- **WHEN** the module UI is at its largest preset and the user presses `+`
- **THEN** the view wraps to the smallest preset

### Requirement: Help modal content reflects live bindings

The help modal opened by `?` SHALL list the keybindings that the handler actually binds for the active view. Every view the modal can describe (Module UI, Source Viewer, Signal-flow Graph, Validation, Optimizer, File Picker) SHALL have a non-empty keybinding table whose rows name the keys the handler dispatches for that surface, including prefix chords (`g s` select-state menu), surface toggles (`\` left-pane split, `h` graph layout mode, `f` dependency filter, `i` influence filter, the module UI's label presentation and fit keys), focus cycling (`Tab`/`Shift+Tab`), and modifier-qualified keys (`Alt+[`/`Alt+]` cable tension) where the handler binds them. Rows SHALL use the key's actual modifier-qualified form rather than an unqualified lookalike.

#### Scenario: Panels table lists split, select-state, and focus keys

- **WHEN** the user opens help from the panels/physical view
- **THEN** the table includes `\` (toggle left-pane vertical split), `g s` (open select-state menu), and `Tab`/`Shift+Tab` (cycle pane focus)

#### Scenario: Graph table lists layout, filter, and tension keys

- **WHEN** the user opens help from the graph surface
- **THEN** the table includes `h` (toggle column/force layout), `f` (dependency filter), `i` (influence filter), `g s` (select-state menu), and `Alt+[`/`Alt+]` (cable tension)

#### Scenario: Physical table lists zoom, pan, and navigation keys

- **WHEN** the user opens help from the physical rack view
- **THEN** the table includes `+`/`-` (zoom presets), the fit key, the label-presentation key, the wheel and middle-drag gestures, arrow keys (pan rack), and `j`/`k` (navigate)

#### Scenario: Every view has a non-empty table

- **WHEN** the user opens help from any view
- **THEN** the modal shows at least the view title and one keybinding row

## ADDED Requirements

### Requirement: Label presentation key on the module UI

While the module UI pane holds focus, `a` SHALL cycle the label presentation through `off` → `auto` → `on` → `off`, report the resulting mode, and persist it as the `[physical] labels` setting. With any other pane focused, `a` SHALL keep its existing meaning, and the help modal's Module UI table SHALL list the new binding.

#### Scenario: Cycling reports the mode

- **WHEN** the module UI is focused and the user presses `a`
- **THEN** the presentation advances to the next mode and the status names it

#### Scenario: The mode survives a restart

- **WHEN** the user selects a presentation and starts the application again
- **THEN** the selection is still in effect

#### Scenario: Help lists the binding

- **WHEN** the user opens help with `?` from the module UI
- **THEN** the Module UI key table lists `a` with its label-presentation meaning
