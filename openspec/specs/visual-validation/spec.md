# visual-validation Specification

## Purpose

Provide an inspectable visual face for the terminal UI, compare it to spec intent, fail the build on face regression, and preserve the face as durable proof in the change archive — so UI evolution is tracked.

## Requirements

### Requirement: Snapshot generation from TestBackend

The system SHALL generate a deterministic ANSI snapshot and an HTML gallery page from the same `TestBackend` buffer for every scenario in the coverage matrix, without a live terminal or pty.

#### Scenario: ANSI snapshot produced

- **WHEN** `cargo test` renders a scenario (fixture, theme, width, viewer state) into a `TestBackend` buffer
- **THEN** an ANSI string (one line per terminal row, trailing empty cells trimmed) is snapshotted via `insta`, and the same buffer is rendered to HTML (one `span` per cell carrying `fg`/`bg`/`bold`/`dim`/`reversed`) under `evidence/gallery/`

#### Scenario: HTML is inspectable side-by-side

- **WHEN** the gallery is opened in a browser
- **THEN** each row shows the same scenario rendered under `classic`, `terminal`, and `mono` side-by-side, with panel borders and kind colors matching `DESIGN.md` tokens

### Requirement: Start-small coverage matrix

The system SHALL cover at least the following scenarios; the matrix is the minimum, not an exhaustive combinatoric expansion.

#### Scenario: Controller-panels face covered

- **WHEN** fixtures `arpeggio1.ini` (P2B8 8 buttons + 2 knobs) and `led_pairs.ini` (mixed boxed/text) are rendered at widths 80 and 120 under each theme
- **THEN** snapshots exist for P2B8 panel, boxed LED border with kind color, and plain text cell distinction

#### Scenario: Viewer-layout and shift face covered

- **WHEN** `source_navigation.ini` is rendered with the embedded source viewer open vs closed, and with `shift1` active (bold colored border + `SHIFT 1 ACTIVE` chip) at width 100
- **THEN** snapshots exist for viewer open/closed and shift-active states

#### Scenario: Fader-column face covered

- **WHEN** a fader-controller fixture (P8S8 Faderbank or M4 Motorfader) is rendered at widths 80 and 120 under each theme
- **THEN** snapshots exist pinning the vertical fader track with its amber LED bar at multiple value levels (e.g. 0%, 50%, 100%), proving position mirrors value.

#### Scenario: LED-association face covered

- **WHEN** a device-LED fixture (M4 RGB touch plates, B32 white-only, master LED) is rendered under each theme
- **THEN** snapshots exist proving each device's LED association renders its correct state and color (M4 RGB, B32 white-only, master→CD channel).

#### Scenario: Adjoined-cell overlap-free across zoom presets

- **WHEN** a physical-view fixture renders at zoom 75%, 100%, 150%, and 200%
- **THEN** a strict no-overlap assertion over all published `component_rects` passes at each preset, and the rendered face is snapshotted per preset.

### Requirement: Strict gate on face regression

The system SHALL fail `cargo test` when any ANSI snapshot differs from the golden.

#### Scenario: Mismatch fails the suite

- **WHEN** a rendering change causes an ANSI snapshot to differ (e.g. label truncation, border color, spacing)
- **THEN** the test run exits non-zero and `cargo insta review` shows the diff; CI treats this as a build failure

#### Scenario: Auto workflow — no manual snapshot command required

- **WHEN** `cargo test` is run without extra flags
- **THEN** snapshots are generated and asserted as part of the normal test run (no separate `make snapshots` step)

### Requirement: Ephemeral worktree, durable archive

Snapshots and the gallery SHALL be ephemeral in the worktree but durable in the OpenSpec archive.

#### Scenario: Worktree stays clean

- **WHEN** snapshots or the gallery are generated locally or in CI
- **THEN** they are ignored by git (`.gitignore` covers `snapshots/` and `evidence/gallery/`) and are not committed to `master`; CI uploads them as an artifact for PR review

#### Scenario: Archive preserves proof

- **WHEN** the change is archived (`openspec archive add-visual-validation`)
- **THEN** the gallery (HTML + ANSI) is copied to `openspec/changes/archive/add-visual-validation/evidence/gallery/` and ships with the archive as durable visual history

### Requirement: Insta-managed golden

The system SHALL use `insta` for golden-file management.

#### Scenario: Review workflow

- **WHEN** an intentional face change is made
- **THEN** the developer runs `cargo insta review` (or `INSTA_UPDATE=always cargo test`) to accept new snapshots locally; acceptance is verified by the next `cargo test` run and the CI artifact, with no commit of golden files required

#### Scenario: CI check mode

- **WHEN** CI runs `cargo insta test --check` (or equivalent)
- **THEN** a snapshot that would be created or updated causes a non-zero exit, and pending snapshots are uploaded as the gallery artifact

### Requirement: Skeleton and full proof rows

The system SHALL render each physical-layout gallery scenario twice — once in skeleton mode and once in full mode — side by side in the matrix, proving the full render coincides with the grid model.

#### Scenario: Gallery shows both presentations

- **WHEN** the gallery matrix is generated
- **THEN** each physical-layout scenario row includes a skeleton-mode frame and a full-mode frame at the same viewport.

#### Scenario: Coincidence is asserted

- **WHEN** both frames are produced for a scenario
- **THEN** the test harness asserts every full-mode element rect equals the corresponding skeleton cell.

### Requirement: Plugin-circuit coverage row

The visual-validation matrix SHALL include a fixture exercising a plugin circuit: a patch whose producing circuit declares `cable_kind` and a `color`, captured across the theme matrix. The snapshot SHALL prove the declared kind/color render (edge kind token, node color) rather than substring inference.

#### Scenario: Plugin-circuit snapshot exists

- **WHEN** the gallery/snapshot harness runs with a plugin-circuit fixture present
- **THEN** the fixture renders across the configured themes and widths, and its snapshots are asserted by the strict insta gate

#### Scenario: No plugin fixture present

- **WHEN** the plugin-circuit fixture is absent from the workspace
- **THEN** the gallery and snapshot runs still pass for the pre-existing fixtures (the plugin row is additive, not a hard requirement for unrelated runs)
