# Spec Delta

## ADDED Requirements

### Requirement: Same-type node grouping

The signal-flow graph SHALL group circuit nodes of the same circuit type when they are spatially clustered in the drawn layout **or** within a small graph distance along cables, by default, whenever the resulting component holds at least `min_group_size` members (default 5). Grouping SHALL be a post-solve view transform: the layout solver, the arrangement in force, determinism per input, and pin anchors SHALL be unchanged, and a group SHALL be placed at its members' bounding-box centre.

A collapsed group SHALL render as one box labelled `<circuit> ×<count>` in that circuit's colour, and SHALL aggregate its members' incoming and outgoing cables into deduplicated group edges; a cable whose both endpoints fall inside the group SHALL NOT be drawn while the group is collapsed.

A group SHALL expose its contents on hover, SHALL expand in place into its member nodes with a group outline on click or `Enter` and collapse again on repeat, and SHALL be isolatable with `o` so the pane draws that group's members together with their dimmed immediate neighbours under a refitted camera. `Shift+O` SHALL toggle grouping for the whole graph.

Selection, diff classification, topology errors, the influence wash, and the dependency filter SHALL each light a group when any of its members matches, under the existing edge-colour precedence (topology error over diff over influence over cable kind).

#### Scenario: A stacked run collapses

- **WHEN** a column contains at least `min_group_size` contiguous nodes of one circuit type
- **THEN** those nodes render as a single box labelled `<circuit> ×<count>`, and the drawn node count falls by the member count minus one

#### Scenario: Cabled same-type nodes group

- **WHEN** same-type circuits are joined through at most two cable hops and that component reaches `min_group_size`
- **THEN** they collapse into one group even when they are not drawn adjacent

#### Scenario: Small components stay individual

- **WHEN** a same-type component holds fewer members than `min_group_size`
- **THEN** every member renders individually and no group box is drawn

#### Scenario: Cables aggregate onto the group

- **WHEN** a group is collapsed
- **THEN** its members' fan-out is deduplicated into distinct group edges, and cables internal to the group are not drawn

#### Scenario: Expand and collapse in place

- **WHEN** the user clicks a collapsed group or presses `Enter` while it is hovered
- **THEN** its member nodes are drawn inside a group outline, and a second click or `Enter` collapses it again

#### Scenario: Hover peeks at contents

- **WHEN** the pointer rests on a collapsed group
- **THEN** a popover names the group's contents and the layout does not change

#### Scenario: Isolate a group

- **WHEN** the user presses `o` with a group hovered or selected
- **THEN** the pane draws that group's members and their dimmed immediate neighbours under a refitted camera, and a second `o` restores the previous view

#### Scenario: Toggle grouping off

- **WHEN** the user presses `Shift+O`
- **THEN** every node renders individually, and a second `Shift+O` restores grouping

#### Scenario: Group lights on member selection

- **WHEN** a circuit that belongs to a collapsed group is selected
- **THEN** that group renders with the selection highlight

#### Scenario: Grouping is deterministic

- **WHEN** the same graph and the same positions are grouped twice with the same settings
- **THEN** both results identify the same groups with the same members and the same drawn edges
