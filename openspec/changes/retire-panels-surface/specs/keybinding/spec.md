# Spec Delta

## MODIFIED Requirements

### Requirement: Help modal content reflects live bindings

The help modal opened by `?` SHALL list the keybindings that the handler actually binds for the active view. Every view the modal can describe (Module UI, Source Viewer, Signal-flow Graph, Validation, Optimizer, File Picker) SHALL have a non-empty keybinding table whose rows name the keys the handler dispatches for that surface, including prefix chords (`g s` select-state menu), surface toggles (`\` left-pane split, `h` graph layout mode, `f` dependency filter, `i` influence filter), focus cycling (`Tab`/`Shift+Tab`), and modifier-qualified keys (`Alt+[`/`Alt+]` cable tension) where the handler binds them. Rows SHALL use the key's actual modifier-qualified form rather than an unqualified lookalike.

#### Scenario: Panels table lists split, select-state, and focus keys

- **WHEN** the user opens help from the panels/physical view
- **THEN** the table includes `\` (toggle left-pane vertical split), `g s` (open select-state menu), and `Tab`/`Shift+Tab` (cycle pane focus)

#### Scenario: Graph table lists layout, filter, and tension keys

- **WHEN** the user opens help from the graph surface
- **THEN** the table includes `h` (toggle column/force layout), `f` (dependency filter), `i` (influence filter), `g s` (select-state menu), and `Alt+[`/`Alt+]` (cable tension)

#### Scenario: Physical table lists zoom, pan, and navigation keys

- **WHEN** the user opens help from the physical rack view
- **THEN** the table includes `+`/`-` (zoom presets), arrow keys (pan rack), and `j`/`k` (navigate)

#### Scenario: Every view has a non-empty table

- **WHEN** the user opens help from any view
- **THEN** the modal shows at least the view title and one keybinding row
