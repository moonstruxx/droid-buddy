## MODIFIED Requirements

### Requirement: Virtual-cable extraction
The system SHALL extract virtual cables from parsed circuit sections, using the embedded DROID schema to decide which parameters produce cables and which consume them:

- A parameter whose key resolves to a catalog output parameter of its circuit creates a cable source when its value is a bare `_NAME` (nothing else).
- A parameter whose key resolves to a catalog input parameter never creates a source; any `_NAME` in its value is a sink reference.
- A parameter whose value references a `_NAME` (bare or inside an arithmetic expression) registers a cable sink, including an output parameter whose value is an expression that reads another cable.
- For a circuit or parameter absent from the catalog, a parameter keyed `output` with a bare `_NAME` value creates a cable source (the prior convention).
- Cables in comment lines (`# output = _X`) are ignored.
- A cable source may fan out to any number of sinks (1 → n topology is valid); `n → 1` is invalid and shall be flagged.

#### Scenario: Cable creation from output param
- **WHEN** a circuit section contains `output = _PULSARCLOCK` and the circuit's catalog marks `output` as an output parameter
- **THEN** a cable source `_PULSARCLOCK` is registered with this circuit as the origin.

#### Scenario: Cable consumption from input param
- **WHEN** a circuit section contains `input = _PULSARCLOCK` (or `frequency = _PULSARCLOCK * 2 - _BASE`)
- **THEN** the cable sink `_PULSARCLOCK` is registered, linking to the source circuit.

#### Scenario: Cable creation from nonstandard output param
- **WHEN** a circuit's catalog output parameter is named `pulse` (not `output`) and the section contains `pulse = _CLK`
- **THEN** a cable source `_CLK` is registered with this circuit as the origin.

#### Scenario: Output expression is a sink
- **WHEN** a circuit's catalog output parameter holds an expression that reads another cable, for example `output = _BASE * 2`
- **THEN** `_BASE` is registered as a sink, not a source.

#### Scenario: Unknown circuit keeps the output convention
- **WHEN** a circuit absent from the catalog contains `output = _X`
- **THEN** `_X` is registered as a source, as before this change.

#### Scenario: Invalid topology n → 1 flagged
- **WHEN** multiple circuits drive `input = _SINGLE_CLOCK` and no circuit outputs `_SINGLE_CLOCK`
- **THEN** the graph highlights an invalid `n → 1` topology error state.
