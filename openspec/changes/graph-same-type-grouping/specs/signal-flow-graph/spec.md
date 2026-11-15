# Spec Delta

## ADDED Requirements

### Requirement: Same-type node grouping

The signal-flow graph SHALL group circuit nodes by circuit type at the default policy: every circuit of one type collapses into a single group node when that type has at least `min_group_size` drawn members (default 5). A run policy SHALL also be selectable, which groups only same-type circuits that are spatially clustered in the drawn layout or within a small graph distance along cables, so an operator can keep positional meaning when they want it.

Grouping SHALL be a post-solve view transform: the layout solver, the arrangement in force, determinism per input, and pin anchors SHALL be unchanged, and a group SHALL be placed at its members' centre.

A collapsed group SHALL render as one box labelled `<circuit> ×<count>` in that circuit's colour. When the members' drawn extent is narrow on at least one axis — within a small multiple of a nominal node width across, or of one node height down — the box SHALL enclose them, so a single column or row is enclosed however long it gets; when both axes are wide the box SHALL be one nominal node-sized box at the members' centroid, so a type spread across the graph draws compactly instead of covering the canvas. A group SHALL aggregate its members' incoming and outgoing cables into deduplicated group edges, and a cable whose both endpoints fall inside the group SHALL NOT be drawn while the group is collapsed.

A group SHALL expose its contents on hover, SHALL expand in place into its member nodes with a group outline on click or `Enter` and collapse again on repeat, and SHALL be isolatable with `o` so the pane draws that group's members together with their dimmed immediate neighbours under a refitted camera; a second `o` SHALL restore the previous view without requiring a hover or a selection. `Shift+O` SHALL toggle grouping for the whole graph.

Selection, diff classification, topology errors, the influence wash, and the dependency filter SHALL each light a group when any of its members matches, under the existing edge-colour precedence (topology error over diff over influence over cable kind).

#### Scenario: A type collapses into one box

- **WHEN** a patch draws at least `min_group_size` circuits of one type, and grouping uses the default type policy
- **THEN** they render as a single box labelled `<circuit> ×<count>` wherever they sit in the layout

#### Scenario: A stacked run collapses

- **WHEN** a column contains at least `min_group_size` contiguous nodes of one circuit type, and grouping uses the run policy
- **THEN** those nodes render as a single box labelled `<circuit> ×<count>`, and the drawn node count falls by the member count minus one

#### Scenario: Cabled same-type nodes group under the run policy

- **WHEN** the run policy is active and same-type circuits are joined through at most two cable hops and that component reaches `min_group_size`
- **THEN** they collapse into one group even when they are not drawn adjacent

#### Scenario: Small populations stay individual

- **WHEN** a circuit type, or a run-policy component, holds fewer members than `min_group_size`
- **THEN** every member renders individually and no group box is drawn

#### Scenario: A spread type draws compactly

- **WHEN** a type group's members are wide on both axes — spread past a small multiple of a nominal node width across and of one node height down
- **THEN** the group draws as one nominal node-sized box at the members' centroid rather than a box enclosing every member

#### Scenario: A real stack is enclosed

- **WHEN** a group's members are narrow on at least one axis — a single column or a single row, however long
- **THEN** the group box encloses them

#### Scenario: Cables aggregate onto the group

- **WHEN** a group is collapsed
- **THEN** its members' fan-out is deduplicated into distinct group edges, and cables internal to the group are not drawn

#### Scenario: Expand and collapse in place

- **WHEN** the user clicks a collapsed group or presses `Enter` while it is hovered
- **THEN** its member nodes are drawn inside a group outline that encloses every one of them, and a second click or `Enter` collapses it again

#### Scenario: Hover peeks at contents

- **WHEN** the pointer rests on a collapsed group
- **THEN** a popover names the group's contents and the layout does not change

#### Scenario: Isolate a group

- **WHEN** the user presses `o` with a group hovered or selected
- **THEN** the pane draws that group's members and their dimmed immediate neighbours under a refitted camera
- **AND** a second `o` restores the previous view without requiring a hover or a selection

#### Scenario: Toggle grouping off

- **WHEN** the user presses `Shift+O`
- **THEN** every node renders individually, and a second `Shift+O` restores grouping

#### Scenario: Group lights on member selection

- **WHEN** a circuit that belongs to a collapsed group is selected
- **THEN** that group renders with the selection highlight

#### Scenario: Grouping is deterministic

- **WHEN** the same graph, positions and settings are grouped twice
- **THEN** both results identify the same groups with the same members and the same drawn edges
