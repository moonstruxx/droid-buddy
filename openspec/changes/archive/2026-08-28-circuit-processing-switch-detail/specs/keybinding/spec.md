## ADDED Requirements

### Requirement: p toggles global processing pause

`p` SHALL toggle global processing pause from anywhere outside the picker (mirroring the global `q`/`l` keys), each toggle producing a status message.

#### Scenario: p pauses globally

- **WHEN** the user presses `p` with processing enabled
- **THEN** processing pauses and the status bar shows the paused state.

#### Scenario: p resumes

- **WHEN** the user presses `p` with processing paused
- **THEN** processing resumes.

### Requirement: x toggles hovered circuit processing in graph

`x` SHALL toggle processing for the hovered graph node's circuit instance while the graph surface is open, rebuilding the graph and recomputing influence. With no node hovered it is a no-op with no status message.

#### Scenario: x acts on hovered node

- **WHEN** the graph surface is open and a node is hovered
- **THEN** `x` toggles that circuit instance's processing state.

#### Scenario: x without hover is a no-op

- **WHEN** the graph surface is open and no node is hovered
- **THEN** `x` changes nothing.