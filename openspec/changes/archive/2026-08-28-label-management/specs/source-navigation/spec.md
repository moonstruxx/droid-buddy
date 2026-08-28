## ADDED Requirements

### Requirement: Source section header circuit label override
The system SHALL render a circuit section header with the circuit-instance label from `LabelStore` when present, otherwise the raw `[circuit]` header, without altering scroll or occurrence indexing.

#### Scenario: Header override
- **WHEN** `circuits."motorfader:12"` is set
- **THEN** the source pane header for that section shows the circuit label while `occurrence_index` still maps to the original span

