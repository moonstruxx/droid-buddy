# patch-validation Specification

## Purpose
TBD - created by archiving change patch-validation. Update Purpose after archive.

## Requirements

### Requirement: Schema-authoritative validation on patch load
The system SHALL validate every loaded patch against the active schema — the merged plugin + embedded schema — on `load_patch` and produce `Vec<ValidationIssue{span: Span, severity: Severity, code: &str, message: String}>` with line-col accurate spans. A circuit defined by a plugin is never "unknown"; its parameters, jacks, and declared `ramsize` participate in the same checks as embedded circuits.

#### Scenario: Unknown circuit is an error with suggestion
- **WHEN** a patch contains a section name that is neither in the embedded `circuits.json` nor in any loaded plugin
- **THEN** the validator emits one `Severity::Error` at the section header span with `code="unknown_circuit"` and a Levenshtein suggestion (`did you mean 'motor'?`)

#### Scenario: Validation runs on every load
- **WHEN** `App::load_patch` is called with any `.ini` path
- **THEN** `validate_patch(&Patch,&Schema)` is invoked against the merged schema and `app.validation_issues` is replaced with the sorted result

#### Scenario: Patch using a plugin circuit validates cleanly
- **WHEN** a patch uses a plugin circuit with valid parameters and within RAM budget
- **THEN** no unknown-circuit or RAM findings are reported for it, and the patch loads

#### Scenario: Plugin circuit keeps RAM validation active
- **WHEN** a patch uses a plugin circuit and the summed ramsize exceeds available memory
- **THEN** the `ram_overflow` check reports an Error — the presence of a plugin circuit must never disable RAM validation (the pre-plugin behavior of skipping RAM checks for any patch containing an unknown circuit no longer applies to plugin circuits)

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

### Requirement: Fail load on Error, modal error list with navigation

Patches with at least one `Severity::Error` SHALL fail to load (`app.patch=None`) and open a modal error list. Patches with only `Warning`/`Hint` SHALL load and SHALL NOT open the modal: the status bar SHALL report the finding count with a hint that `e` opens the list, so a warnings-only patch keeps its normal key handling (including app chords such as `g g`). In every case the modal lists all issues sorted by `(line,col)` and is reachable on demand with `e`.

#### Scenario: Error blocks patch display

- **WHEN** validation produces ≥1 Error
- **THEN** panels remain empty, `showing_validation=true`, status `Load failed: N errors — press 'e' to view`, and the modal lists all issues sorted by `(line,col)` with `L{line}:{col} [E/W/H] [code] message`

#### Scenario: Warnings load without opening the modal

- **WHEN** validation produces only `Warning`/`Hint` findings
- **THEN** the patch loads, `showing_validation=false`, the status bar reports the finding count with a hint that `e` opens the list, and app chords (`g g`, `l`, `?`) keep working until the user presses `e`

#### Scenario: Modal navigation jumps to source

- **WHEN** the modal is open and user presses `Enter` on a selected issue
- **THEN** the source viewer scrolls to `issue.span.line` and the issue's span is highlighted; `j/k` moves `validation_cursor`, `Esc`/`e` dismisses the modal

### Requirement: Required parameters satisfied by documented alternatives

The system SHALL satisfy an `essential == 2` input declared as one of several alternative inputs by the DROID manual when at least one of its documented alternatives carries a value in the same patch section. An alternative whose assignment is empty, or whose value is the `unpatched` placeholder, SHALL NOT count: it states the absence of an input. The alternatives SHALL come from an explicit table in `src/validation.rs`, each row citing its manual basis, and SHALL NOT be inferred from a param's `default`, its type, or its name.

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

#### Scenario: An unwritten alternative value keeps the warning
- **WHEN** a `[contour]` section writes `trigger` with no value (`trigger =`) or with `trigger = unpatched`, and no `gate`
- **THEN** `missing_required` is emitted for `gate`

#### Scenario: Drum patch family validates without these false positives
- **WHEN** `patches_remote/_droid_mpfs5drum.ini` (311 sections) is validated
- **THEN** it yields zero `missing_required` findings and zero findings of any other code
