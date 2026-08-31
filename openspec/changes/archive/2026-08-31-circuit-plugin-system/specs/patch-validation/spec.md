# Patch Validation — delta

## MODIFIED Requirements

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