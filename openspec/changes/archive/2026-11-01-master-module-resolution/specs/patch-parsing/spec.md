# Spec Delta

## ADDED Requirements

### Requirement: Master-module declaration from the preamble

The system SHALL read the DROID Forge's master selection from the patch preamble and
expose it as an optional declaration on the parsed patch. The declaration is the
comment `# LABELS: master=NN` in the leading comment block, before the first section
header, where `NN` is `16` for MASTER or `18` for MASTER18. The first valid
declaration in the block SHALL win. Any other value SHALL be ignored — no finding, no
change to the rest of the parse — because the field belongs to the Forge, not to this
app. The declaration line SHALL NOT be treated as a hardware label: `LABELS` is not a
hardware token, so it SHALL NOT appear in the patch's preamble label map and no
element's label SHALL resolve from it.

#### Scenario: MASTER18 is declared

- **WHEN** a patch preamble contains `# LABELS: master=18`
- **THEN** the parsed patch reports a MASTER18 declaration

#### Scenario: MASTER is declared

- **WHEN** a patch preamble contains `# LABELS: master=16`
- **THEN** the parsed patch reports a MASTER declaration

#### Scenario: A patch without a declaration

- **WHEN** a patch preamble contains no `# LABELS: master=` line
- **THEN** the parsed patch reports no declaration, and every other parse result is unchanged

#### Scenario: An unrecognised value is ignored

- **WHEN** a patch preamble contains `# LABELS: master=20`
- **THEN** the parsed patch reports no declaration and no finding

#### Scenario: The declaration is not a label

- **WHEN** a patch contains `# LABELS: master=18` in its preamble
- **THEN** the preamble label map does not contain a `LABELS` entry, and no element's label resolves to `master=18`

#### Scenario: A declaration after the first section is not read

- **WHEN** `# LABELS: master=18` appears after the first `[section]` header
- **THEN** the patch reports no declaration
