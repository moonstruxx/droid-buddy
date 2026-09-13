# visual-validation Specification

## MODIFIED Requirements

### Requirement: Snapshot generation from TestBackend

The system SHALL generate a deterministic ANSI snapshot and an HTML gallery page from the same `TestBackend` buffer for every scenario in the coverage matrix, without a live terminal or pty, and SHALL complete the full matrix within the named render budget of the performance gate.

#### Scenario: ANSI snapshot produced

- **WHEN** `cargo test` renders a scenario (fixture, theme, width, viewer state) into a `TestBackend` buffer
- **THEN** an ANSI string (one line per terminal row, trailing empty cells trimmed) is snapshotted via `insta`, and the same buffer is rendered to HTML (one `span` per cell carrying `fg`/`bg`/`bold`/`dim`/`reversed`) under `evidence/gallery/`

#### Scenario: HTML is inspectable side-by-side

- **WHEN** the gallery is opened in a browser
- **THEN** each row shows the same scenario rendered under `classic`, `terminal`, and `mono` side-by-side, with panel borders and kind colors matching `DESIGN.md` tokens

#### Scenario: Matrix completes within render budget

- **WHEN** the full gallery matrix runs under the performance gate
- **THEN** every scenario and theme completes within the named render budget, and the elapsed time is reported