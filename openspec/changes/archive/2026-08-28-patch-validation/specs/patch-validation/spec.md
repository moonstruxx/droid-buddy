# patch-validation Specification

## ADDED Requirements

### Requirement: Schema-authoritative validation on patch load
The system SHALL validate every loaded patch against the authoritative `droid-lsp` schema (`circuits.json` via `src/schema.rs`) on `load_patch` and produce `Vec<ValidationIssue{span: Span, severity: Severity, code: &str, message: String}>` with line-col accurate spans.

#### Scenario: Unknown circuit is an error with suggestion
- **WHEN** a patch contains a section name not in the embedded `circuits.json`
- **THEN** the validator emits one `Severity::Error` at the section header span with `code="unknown_circuit"` and a Levenshtein suggestion (`did you mean 'motor'?`)

#### Scenario: Validation runs on every load
- **WHEN** `App::load_patch` is called with any `.ini` path
- **THEN** `validate_patch(&Patch,&Schema)` is invoked and `app.validation_issues` is replaced with the sorted result

### Requirement: Nine LSP-equivalent checks with typed severity
The system SHALL implement all 9 checks from `droid-lsp/src/diagnostics.ts` with identical severity mapping.

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
- **WHEN** a circuit requires a param (`essential==2`) and it is absent (including `count`-expanded required params)
- **THEN** the validator emits `Severity::Warning` at the section `header_span` (`code="missing_required"`)

#### Scenario: Undefined cable is a warning, duplicate definition is a warning, unused is a hint
- **WHEN** a `_CABLE` value is referenced but never defined as an `output` / `WHEN` the same `_cable` is defined in two circuits / `WHEN` a `_cable` is defined but never referenced
- **THEN** emits `Warning` / `Warning` at second def value span / `Hint` at def value span respectively

#### Scenario: RAM budget overflow is an error
- **WHEN** sum of `ramsize` for all sections (via Schema) exceeds `available_memory`
- **THEN** emits `Severity::Error` at `Span{line:0}` (`code="ram_overflow"`), message includes `used/available`

### Requirement: Fail load on Error, modal error list with navigation
Patches with at least one `Severity::Error` SHALL fail to load (`app.patch=None`) and open a modal error list; patches with only `Warning`/`Hint` SHALL load but also open the modal.

#### Scenario: Error blocks patch display
- **WHEN** validation produces ≥1 Error
- **THEN** panels remain empty, `showing_validation=true`, status `Load failed: N errors — press 'e' to view`, and the modal lists all issues sorted by `(line,col)` with `L{line}:{col} [E/W/H] [code] message`

#### Scenario: Modal navigation jumps to source
- **WHEN** the modal is open and user presses `Enter` on a selected issue
- **THEN** the source viewer scrolls to `issue.span.line` and the issue's span is highlighted; `j/k` moves `validation_cursor`, `Esc`/`e` dismisses the modal

