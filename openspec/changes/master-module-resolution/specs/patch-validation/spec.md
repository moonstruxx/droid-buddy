# Spec Delta

## MODIFIED Requirements

### Requirement: Nine LSP-equivalent checks with typed severity

The system SHALL implement all 9 checks from `droid-lsp/src/diagnostics.ts` with identical severity mapping, with one documented local extension: a required (`essential == 2`) parameter that the manual documents as one of several alternative inputs is satisfied by any of its alternatives being present in the same section. The RAM budget check SHALL judge the summed circuit RAM against the budget of the master model resolved for the patch, and SHALL emit at most one `ram_overflow` finding naming that model.

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
- **WHEN** sum of `ramsize` for all sections (via Schema) exceeds the resolved master model's budget
- **THEN** exactly one `Severity::Error` is emitted at `Span{line:0}` (`code="ram_overflow"`), and its message names that master and its `used/available` figures — not one finding per schema master

#### Scenario: A patch inside the MASTER18 budget but outside MASTER's still reports once
- **WHEN** a patch with no declaration exceeds `master16`'s budget but fits `master18`'s
- **THEN** one `ram_overflow` finding names MASTER

## ADDED Requirements

### Requirement: Master-module resolution and fit

Every patch SHALL resolve to exactly one master model — MASTER or MASTER18 — and the
validation result SHALL report every use the resolved model cannot host. Resolution
SHALL prefer the patch's declared master; without a declaration it SHALL infer the
single model that can host the patch when exactly one can; otherwise it SHALL use
MASTER as the default. The capability rules are the hardware's: an input jack `I3`
or higher requires MASTER; `I1` or `I2` bound to a `cv`-typed parameter requires
MASTER, because MASTER18 reads those two jacks as logic levels only; a `vcotuner` or
`sinfonionlink` section requires MASTER18; `O1` through `O8` are hostable by both.

A use the resolved model cannot host SHALL be reported as `master_conflict` with
`Severity::Warning` at the offending entry's span, naming the model that would host
it, and the patch SHALL still load — the declaration is authoritative for what is
drawn, and a conflict is a problem to inspect, not a refusal. When no model can host
every use — a patch requiring MASTER for its input jacks and MASTER18 for a circuit —
the validator SHALL emit `no_master_fits` with `Severity::Error`, which gates the load
like every other Error, and the message SHALL name both conflicting uses. The load
status SHALL name the resolved model and whether it was declared or inferred.

#### Scenario: A declared MASTER18 patch that reads CV inputs is reported and still loads

- **WHEN** a patch declares `master=18` and binds a `cv`-typed parameter to `I3`
- **THEN** one `master_conflict` Warning names `I3` and MASTER, and the patch loads

#### Scenario: A declared MASTER18 patch using a MASTER18-only circuit is clean

- **WHEN** a patch declares `master=18` and contains a `[vcotuner]` section
- **THEN** no `master_conflict` is reported for that section

#### Scenario: A MASTER patch using a MASTER18-only circuit is reported

- **WHEN** a patch declares `master=16` and contains a `[sinfonionlink]` section
- **THEN** one `master_conflict` Warning names the section and MASTER18, and the patch loads

#### Scenario: Gate inputs on MASTER18 are not a conflict

- **WHEN** a patch declares `master=18` and binds a `gate`- or `trigger`-typed parameter to `I1` or `I2`
- **THEN** no `master_conflict` is reported

#### Scenario: Silence infers the only model that fits

- **WHEN** a patch declares no master and contains a `[vcotuner]` section with no input jack above `I2` used as CV
- **THEN** the resolved model is MASTER18, reported as inferred, with no findings

#### Scenario: Silence defaults to MASTER

- **WHEN** a patch declares no master and uses only jacks and circuits both models host
- **THEN** the resolved model is MASTER, reported as the default

#### Scenario: No master can host the patch

- **WHEN** a patch uses `I6` as a CV input and contains a `[vcotuner]` section
- **THEN** one `no_master_fits` Error names both uses and the patch fails to load

#### Scenario: The load status names the model and its source

- **WHEN** a patch declaring `master=18` loads with only warnings
- **THEN** the status names `MASTER18` and says the model was declared
