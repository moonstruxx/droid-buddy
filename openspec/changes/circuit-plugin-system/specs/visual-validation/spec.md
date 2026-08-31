# Visual Validation — delta

## MODIFIED Requirements

### Requirement: Plugin-circuit coverage row
The visual-validation matrix SHALL include a fixture exercising a plugin circuit: a patch whose producing circuit declares `cable_kind` and a `color`, captured across the theme matrix. The snapshot SHALL prove the declared kind/color render (edge kind token, node color) rather than substring inference.

#### Scenario: Plugin-circuit snapshot exists
- **WHEN** the gallery/snapshot harness runs with a plugin-circuit fixture present
- **THEN** the fixture renders across the configured themes and widths, and its snapshots are asserted by the strict insta gate

#### Scenario: No plugin fixture present
- **WHEN** the plugin-circuit fixture is absent from the workspace
- **THEN** the gallery and snapshot runs still pass for the pre-existing fixtures (the plugin row is additive, not a hard requirement for unrelated runs)