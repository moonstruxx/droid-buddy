# Performance-View Specification

## Purpose

Exploded-label performance view for the module UI: a compact centered rack
with big side-bound labels, so every element name is readable at a glance.
DROID hardware supports no dynamic labels, so the tree-guessed labels are the
headline content of this view.

## Requirements

### Requirement: Open the performance view with `p`

The Physical pane SHALL bind `p` (no modifiers) to opening the performance
view: the rack renders compact and centered (~1/4 of the pane) and every
element label renders in the surrounding field. `p` on any other pane keeps
its existing meaning.

#### Scenario: Entering the performance view

- WHEN the Physical pane holds focus and the user presses `p`
- THEN the pane shows the compact rack plus the label field, with the status
  naming the view.

#### Scenario: `p` elsewhere is unchanged

- WHEN any non-Physical pane holds focus and the user presses `p`
- THEN processing pause (or pin on the graph pane) happens as before.

### Requirement: Labels surround, never cover

Labels SHALL render only in the field outside the rack rect, each joined to
its host cell edge by a leader line. No label may overlap the rack.

#### Scenario: Rack stays pure hardware

- WHEN the performance view is open
- THEN every drawn label rect is disjoint from the rack rect and every label
  has a leader line to its host cell.

### Requirement: Side-bound stable placement

Each label SHALL be assigned one side (over / under / left / right) by host
position relative to the rack center and keep that side across frames;
labels on one side stack along it without overlapping each other.

#### Scenario: Stable sides across frames

- WHEN two consecutive frames paint the same patch and selection
- THEN every label keeps its side and slot.

### Requirement: Guessed-first label content

Callouts SHALL resolve store-defined → tree-guessed → derived token, reusing
the existing resolution chain, so unlabeled circuits show their inherited
names.

#### Scenario: Unlabeled circuit shows its guess

- WHEN a circuit has no store label but the guess cache names it
- THEN its callout shows the guessed name.

### Requirement: Live state and reset

Activating an element SHALL update its callout's state readout live; a reset
key SHALL restore all element states to rest without touching
selection/focus/shift. `Esc` SHALL leave the performance view.

#### Scenario: Reset restores rest state

- WHEN elements were toggled and the user presses the reset key
- THEN all element states read rest while selection and focus are unchanged.

### Requirement: Performance-view zoom

While the performance view is open, the module UI's zoom SHALL scale the compact
rack: the drawn rack SHALL be the compact presentation (`fit × 0.5`) multiplied by
the current zoom, clamped to the whole-rack fit, so the rack grows from a quarter of
the pane up to the whole pane and never overflows it. `+`/`-` and the wheel SHALL
therefore change the performance view's size, and no panning SHALL be needed or
provided. The status line SHALL report the zoom in percent.

Entering the performance view SHALL NOT reset the zoom; leaving it SHALL leave the
zoom unchanged, so the module UI and the performance view share one zoom setting.

#### Scenario: Zoom enlarges the compact rack

- WHEN the performance view is open at 100 % and the user presses `+`
- THEN the rack is drawn larger (and the callouts with it), and the status reports the new zoom

#### Scenario: The clamp stops at the whole-rack fit

- WHEN the zoom is already high enough for the rack to fill the pane and the user presses `+` again
- THEN the rack does not grow past the pane

#### Scenario: Zoom is shared with the module UI

- WHEN the user zooms inside the performance view and then leaves it
- THEN the module UI pane keeps that zoom

### Requirement: Labels scale with the view zoom

Performance callouts SHALL scale with the performance-view zoom by default: their
effective size SHALL be the base label size times the zoom relative to the compact
presentation, with 100 % zoom producing the 14 pt presentation the view ships today.
The user SHALL be able to adjust the base label scale independently with
`Shift`+`+`/`-` (see "Performance-view label-scale keys"), and 100 % zoom with 100 %
scale SHALL be the compact presentation. The scale SHALL be persisted as
`[labels] performance_scale`.

Label annotations outside the performance view and the hover reveal SHALL keep their
fixed size; only the performance callouts follow the zoom and the scale.

Label placement SHALL stay consistent with the painted size: the same effective size
SHALL drive both the placement budget and the drawn text, so a scaled label is never
placed in a rect measured for a different size.

#### Scenario: Labels grow with the zoom

- WHEN the user presses `+` in the performance view
- THEN every callout's text grows with the rack, and the leader lines still join each label to its host cell

#### Scenario: The base scale is independent

- WHEN the user presses `Shift`+`-` twice at a fixed zoom
- THEN the labels shrink while the rack keeps its size

#### Scenario: 100 % is today's presentation

- WHEN the zoom is 100 % and the base scale is 100 %
- THEN callouts draw at 14 pt, exactly as before this change

#### Scenario: Placement matches the painted size

- WHEN the labels are scaled up
- THEN no label rect overlaps another label or the rack, and none was measured at the unscaled size

#### Scenario: The scale survives a restart

- WHEN the user sets the base scale to 115 % and restarts the app
- THEN the performance view opens with 115 %
