## Purpose

Exploded-label performance view for the module UI: a compact centered rack
with big side-bound labels, so every element name is readable at a glance.
DROID hardware supports no dynamic labels, so the tree-guessed labels are the
headline content of this view.

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
