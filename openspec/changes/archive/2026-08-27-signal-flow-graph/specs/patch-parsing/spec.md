## ADDED Requirements

### Requirement: Virtual-cable/signal-flow extraction

The system SHALL extract virtual cables from parsed circuit section values, in addition to the existing hardware-token extraction:

- `output = _NAME` on a circuit creates a cable source named `_NAME`.
- Any circuit param `= _NAME` (including expression-embedded tokens like `input = _X * 2 + _Y`) references a cable sink named `_NAME`.
- Cable definitions in comment lines (prefixed with `#`) SHALL be ignored; only real section param values are extracted.
- A cable source may fan out to any number of sinks (valid topology: 1 source → n sinks). The topology `n → 1` (multiple sources driving one sink) is invalid and shall be flagged as an error.

#### Scenario: Cable source from output param

- **WHEN** a circuit section contains `output = _PULSARCLOCK`
- **THEN** a cable source `_PULSARCLOCK` is registered with this circuit as the origin, and no scenario beyond this is required for this single requirement.

#### Scenario: Cable sink from expression-embedded param

- **WHEN** a circuit section contains `input = _ENV1_DECAY_POT_ABSBIPOLAR * -1 + _DECAY_MIN`
- **THEN** the tokens `_ENV1_DECAY_POT_ABSBIPOLAR` and `_DECAY_MIN` are each recognized as cable references; the parser records their roles as potential sources or sinks based on context.

#### Scenario: Invalid n → 1 topology flagged

- **WHEN** multiple circuits have `input = _SINGLE_CLOCK` and no circuit in the patch outputs `_SINGLE_CLOCK`
- **THEN** the parser flags an invalid `n → 1` topology error, which the graph view renders as a topology-error state.