## MODIFIED Requirements

### Requirement: Nine LSP-equivalent checks with typed severity
The system SHALL implement all 9 checks from `droid-lsp/src/diagnostics.ts` with identical severity mapping, with one documented local extension: a required (`essential == 2`) parameter that the manual documents as one of several alternative inputs is satisfied by any of its alternatives being present in the same section.

#### Scenario: Duplicate parameter is a warning at second occurrence
- **WHEN** a section contains the same param key twice (case-insensitive, expanded names)
- **THEN** the validator emits `Severity::Warning` at the second occurrence's value span (`code="duplicate_param"`)

#### Scenario: Unknown parameter is an error
- **WHEN** a circuit section contains a param not in `getParamNames(circuit)` (after `prefix/count/start_at` expansion)
- **THEN** the validator emits `Severity::Error` at that entry's value span (`code="unknown_param"`)

#### Scenario: Invalid jack is a warning at value
- **WHEN** a jack-typed param value is not in `JACK_TABLE` nor is a valid number/ `_cable`
- **THEN** the validator emits `Severity::Warning` at the value span (`code="invalid_jack"`)

#### Scenario: Missing required parameter is a warning at header
- **WHEN** a circuit requires a param (`essential==2`), it is absent (including `count`-expanded required params), and no documented alternative for it is present in that section
- **THEN** the validator emits `Severity::Warning` at the section `header_span` (`code="missing_required"`)

#### Scenario: Undefined cable is a warning, duplicate definition is a warning, unused is a hint
- **WHEN** a `_CABLE` value is referenced but never defined as an `output` / `WHEN` the same `_cable` is defined in two circuits / `WHEN` a `_cable` is defined but never referenced
- **THEN** emits `Warning` / `Warning` at second def value span / `Hint` at def value span respectively

#### Scenario: RAM budget overflow is an error
- **WHEN** sum of `ramsize` for all sections (via Schema) exceeds `available_memory`
- **THEN** emits `Severity::Error` at `Span{line:0}` (`code="ram_overflow"`), message includes `used/available`

## ADDED Requirements

### Requirement: Required parameters satisfied by documented alternatives

The system SHALL satisfy an `essential == 2` input declared as one of several alternative inputs by the DROID manual when at least one of its documented alternatives is present in the same patch section. The alternatives SHALL come from an explicit table in `src/validation.rs`, each row citing its manual basis, and SHALL NOT be inferred from a param's `default`, its type, or its name.

#### Scenario: Taptempo LFO is not missing hz
- **WHEN** a `[lfo]` section sets `taptempo` (or `rate`) and no `hz`
- **THEN** no `missing_required` finding for `hz` is emitted

#### Scenario: LFO with no speed source still reports hz
- **WHEN** a `[lfo]` section sets neither `hz`, `taptempo` nor `rate`
- **THEN** one `missing_required` finding for `hz` is emitted

#### Scenario: Trigger-only gatetool is not missing inputgate
- **WHEN** a `[gatetool]` section sets `inputtrigger` or `inputedge` and no `inputgate`
- **THEN** no `missing_required` finding for `inputgate` is emitted

#### Scenario: Contour driven by trigger is not missing gate
- **WHEN** a `[contour]` section sets `trigger` and no `gate`
- **THEN** no `missing_required` finding for `gate` is emitted

#### Scenario: Compare with any branch is not missing ifequal
- **WHEN** a `[compare]` section sets any of `ifgreater`, `ifless` or `else` and no `ifequal`
- **THEN** no `missing_required` finding for `ifequal` is emitted

#### Scenario: An alternative does not excuse a different required param
- **WHEN** a `[compare]` section sets `ifgreater` but omits `input`
- **THEN** `missing_required` is emitted for `input` (and not for `ifequal`)

#### Scenario: Empty alternative set keeps the warning
- **WHEN** a `[gatetool]` section sets none of `inputgate`, `inputtrigger`, `inputedge`
- **THEN** `missing_required` is emitted for `inputgate`

#### Scenario: Drum patch family validates without these false positives
- **WHEN** `patches_remote/_droid_mpfs5drum.ini` (311 sections) is validated
- **THEN** it yields zero `missing_required` findings and zero findings of any other code
