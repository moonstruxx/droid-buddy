## ADDED Requirements

### Requirement: Controller panel component labels use overlay fallback
The system SHALL render each HW component cell label as `Patch::display_label(token, active_shift)` with the overlay fallback chain, preserving cell geometry and hit-testing.

#### Scenario: Panel shows store label
- **WHEN** active shift is Group 2 and `hw."B3.17".2` is set
- **THEN** the panel cell for `B3.17` shows the store label for layer 2, and `component_rects` for hit-testing is unchanged

