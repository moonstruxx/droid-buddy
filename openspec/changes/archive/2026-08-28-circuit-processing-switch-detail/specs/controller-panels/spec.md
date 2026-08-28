## ADDED Requirements

### Requirement: Panels render dim while processing paused

While global processing is paused, the panel area SHALL render with dimmed styling (all panel content, including boxes and text), the status bar SHALL show `PROCESSING PAUSED`, and a status message SHALL read `Processing paused (p to resume)`. Geometry, hit-testing, and module grouping are unchanged while paused.

#### Scenario: Paused panels dim

- **WHEN** processing is paused
- **THEN** the panel main area renders dimmed while the header and status bars render normally.

#### Scenario: Resume un-dims

- **WHEN** processing is resumed
- **THEN** panels render exactly as before pausing.