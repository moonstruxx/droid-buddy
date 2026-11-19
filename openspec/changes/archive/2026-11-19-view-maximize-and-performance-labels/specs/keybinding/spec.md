# Spec Delta

## ADDED Requirements

### Requirement: Performance-view label-scale keys

While the module UI shows the performance view, `Shift`+`+` and `Shift`+`-` SHALL
step the callout labels' base scale by 5 % each press, clamped to 50 % and 400 %,
and the status line SHALL report the resulting scale. At the clamp the key SHALL
be a no-op that still reports the limit. The same keys SHALL keep their existing
meaning everywhere else: `Shift`+`+`/`-` on the graph pane scales the other
pane's zoom, and on any other pane they do nothing.

Plain `+`/`-` SHALL keep the zoom meaning they have in that view (see
"Performance-view zoom"); the shifted pair is the only label-scale binding.

#### Scenario: Shift+plus enlarges the labels

- **WHEN** the performance view is open and the user presses `Shift`+`+`
- **THEN** every callout's base scale grows by 5 % and the status reports the new percentage

#### Scenario: Shift+minus shrinks the labels

- **WHEN** the performance view is open at 100 % and the user presses `Shift`+`-`
- **THEN** every callout's base scale drops to 95 % and the status reports it

#### Scenario: The clamp holds

- **WHEN** the base scale is 400 % and the user presses `Shift`+`+` again
- **THEN** the scale stays at 400 % and the status reports the limit

#### Scenario: Other surfaces are unchanged

- **WHEN** the graph pane or the source viewer holds focus and the user presses `Shift`+`+`
- **THEN** the performance label scale does not change and the focused pane keeps its own meaning for the key

## MODIFIED Requirements

### Requirement: Physical-view navigation keys

The system SHALL provide keys to pan the physical view when the rack overflows the pane, to change zoom, to fit the rack to the pane, and to toggle the skeleton reference mode. Overflow SHALL be judged against the pane the module UI is actually drawn in, not against a stored viewport size, so panning works whenever the drawn rack exceeds the pane on the pressed axis. The system SHALL provide two fit keys for the module UI pane: `Shift+F` SHALL apply the label fit (the larger of the whole-rack fit and the zoom every written label needs, clamped to the supported range) and `g f` SHALL apply the rack fit (the whole-rack fit alone). The status report SHALL name which fit applied.

`f` no longer applies a fit: it is the band's maximize (see "Pane layout keys"), and `Shift+F` is free for the labels fit because shifted glyphs are distinct keys. The three fit scenarios below keep their historical headings; read `Shift+F` in the first two as the labels fit and `g f` in the third as the rack fit.

#### Scenario: Pan keys move the viewport

- **WHEN** the rack is larger than the pane and the user presses a pan key
- **THEN** the viewport offset moves in the corresponding direction without changing zoom, and the status reports the offset

#### Scenario: Pan keys are inert on a fitting rack

- **WHEN** the drawn rack fits the pane on an axis and the user presses the pan key for that axis
- **THEN** the view does not move and the key falls through to its navigation meaning

#### Scenario: Fit key applies the label fit

- **WHEN** the user has zoomed and panned and then presses `Shift+F`
- **THEN** the view applies the label fit — every written label fits its cell — and the status reports `fit(labels)`

#### Scenario: Fit key returns to the fitted view

- **WHEN** the user has zoomed and panned and then presses a fit key
- **THEN** the rack is scaled to that fit and centred, and the status reports which fit applied

#### Scenario: Shift+F applies the rack fit

- **WHEN** the user presses `g f` with a module UI pane open
- **THEN** the view fits the whole rack to the pane, centring it, and the status reports `fit(rack)`

#### Scenario: g f without a module UI pane

- **WHEN** the user presses `g f` and no pane shows the module UI
- **THEN** the band is unchanged and the status reports that no module UI pane is open

#### Scenario: f no longer fits

- **WHEN** the module UI pane holds focus and the user presses `f`
- **THEN** the band maximizes and the zoom does not change

#### Scenario: Skeleton toggle switches presentation

- **WHEN** the user presses the skeleton-toggle key
- **THEN** the main view switches between full and skeleton presentation of the same layout, and back.

### Requirement: Help modal content reflects live bindings

The help modal opened by `?` SHALL list the keybindings that the handler actually binds for the active view. Every view the modal can describe (Module UI, Source Viewer, Signal-flow Graph, Validation, Optimizer, File Picker) SHALL have a non-empty keybinding table whose rows name the keys the handler dispatches for that surface, including prefix chords (`g s` select-state menu, `g f` rack fit), surface toggles (`\` left-pane split, `h` graph layout mode, `Shift+F` dependency filter, `i` influence filter, the module UI's label presentation and `Shift+F` labels fit), focus cycling (`Tab`/`Shift+Tab`), the band's `f`/`z` maximize, and modifier-qualified keys (`Alt+[`/`Alt+]` cable tension, `Shift`+`+`/`-` label scale) where the handler binds them. Rows SHALL use the key's actual modifier-qualified form rather than an unqualified lookalike.

#### Scenario: Panels table lists split, select-state, and focus keys

- **WHEN** the user opens help from the panels/physical view
- **THEN** the table includes `\` (toggle left-pane vertical split), `g s` (open select-state menu), and `Tab`/`Shift+Tab` (cycle pane focus)

#### Scenario: Graph table lists layout, filter, and tension keys

- **WHEN** the user opens help from the graph surface
- **THEN** the table includes `h` (toggle column/force layout), `Shift+F` (dependency filter), `i` (influence filter), `g s` (select-state menu), and `Alt+[`/`Alt+]` (cable tension)

#### Scenario: Physical table lists zoom, pan, and navigation keys

- **WHEN** the user opens help from the physical rack view
- **THEN** the table includes `+`/`-` (zoom presets), `Shift+F` (labels fit), `g f` (rack fit), `f`/`z` (maximize), the label-presentation key, the wheel and middle-drag gestures, arrow keys (pan rack), and `j`/`k` (navigate)

#### Scenario: Every view has a non-empty table

- **WHEN** the user opens help from any view
- **THEN** the modal shows at least the view title and one keybinding row

### Requirement: Pane layout keys

The system SHALL bind `z` and `f` to toggle the maximize of the focused pane, `Alt+b` to exchange the big pane's view with the focused pane's view, and `Alt+s` to exchange the two small panes. Both maximize keys SHALL mean the same thing with the same clearing rules (a second press, a focus change, or `Esc` restores the arrangement) and SHALL work in every pane and for every view. These keys SHALL apply in the main band and SHALL NOT fire while the file picker, the validation modal, the label overlay, or the help modal has focus. The help modal tables SHALL list all four keys.

#### Scenario: z toggles maximize
- **WHEN** the user presses `z`
- **THEN** the focused pane maximizes, and pressing `z` again restores the split

#### Scenario: f toggles maximize
- **WHEN** the user presses `f` with any pane focused
- **THEN** the focused pane maximizes, and pressing `f` again restores the split

#### Scenario: f clears like z
- **WHEN** a pane is maximized with `f` and the user presses `Tab` or `Esc`
- **THEN** the maximize clears and the arrangement returns, exactly as after `z`

#### Scenario: Alt+b swaps big with the focused pane
- **WHEN** the user presses `Alt+b` with a small pane focused
- **THEN** the big pane's view and the focused pane's view exchange places

#### Scenario: Alt+s swaps the small panes
- **WHEN** the user presses `Alt+s`
- **THEN** the two small panes exchange views

#### Scenario: Overlays take priority
- **WHEN** the help modal is open and the user presses `z`
- **THEN** the maximize does not toggle and the modal handles the key

#### Scenario: The picker keeps f
- **WHEN** the file picker holds focus and the user presses `f`
- **THEN** the picker toggles the favourite on the highlighted entry and the band is unchanged

#### Scenario: Help lists the layout keys
- **WHEN** the user opens help with `?`
- **THEN** the table lists `z`, `f`, `Alt+b`, and `Alt+s`
