# render-outlier-detection Specification

## Purpose

Detect, before the user notices, when a DROID patch's rendering silently degrades at the user's terminal width or palette — boxed cells falling back to unboxed two-line rendering, clipped panels, hidden source sidebar/minimap, or failing mono contrast — and surface it as a status hint, plus flag it in the gallery CI matrix. The scoring is deterministic, pure, and runs entirely in-process from a distilled decision table; it never gates patch loading.

## Requirements

### Requirement: Render-metrics extraction

The system SHALL compute deterministic render metrics for a patch at a given terminal width and theme, derived from the renderer's own layout constants (component/panel/module counts, minimum width needed, overflow columns, boxed→unboxed fallback rate, sidebar/minimap hidden flags, and mono contrast minima between co-occurring tokens), without rendering a frame.

#### Scenario: Extract metrics for a patch at a width
- **WHEN** a patch and a terminal width are provided
- **THEN** the extractor returns the feature set for that (patch, width, theme) combination deterministically.

#### Scenario: Feature set is stable
- **WHEN** extraction runs twice for the same (patch, width, theme)
- **THEN** the feature sets are identical.

### Requirement: Embedded distilled scorer

The system SHALL score render features with a distilled decision table embedded in the binary via `include_str!` (≤ a few KB), mirroring the rack-wiring-outlier scorer pattern. On a miss the scorer SHALL fall back to a heuristic baseline (minimum-width heuristic) rather than failing.

#### Scenario: Scored outlier is flagged
- **WHEN** a patch's features at a width exceed the table's degraded band
- **THEN** the render is flagged degraded with the recommendation computed from the table.

#### Scenario: Table miss falls back
- **WHEN** the feature vector has no table entry
- **THEN** the heuristic baseline decides, and the result is the same as the baseline rule.

### Requirement: Invariant guards

The system SHALL never flag a render at or above the patch's native-fit width, SHALL never flag a render for which the heuristic baseline is clean, and SHALL never gate patch loading or block rendering.

#### Scenario: Native fit is never flagged
- **WHEN** a patch renders at a width at or above its native fit
- **THEN** no degraded warning is produced.

#### Scenario: Loading is never blocked
- **WHEN** a patch's render is predicted degraded
- **THEN** the patch still loads and renders normally; only a status hint is added.

### Requirement: Status-hint surface

The system SHALL surface a predicted-degraded render via a status message using a dedicated theme token (e.g. `Renders degraded at N cols — use ≥ M cols or reduce scale`) whenever a patch is loaded and its render at the current terminal size/theme is predicted degraded.

#### Scenario: Degraded render shows a hint
- **WHEN** a patch loads at a width where its render is predicted degraded
- **THEN** the status bar shows the render-outlier hint in the `render_outlier_warning` token.

#### Scenario: Healthy render shows no hint
- **WHEN** a patch loads at a width where its render is predicted healthy
- **THEN** no render-outlier hint is shown.

### Requirement: Gallery-CI render-outlier flag

The system SHALL flag scenarios in the visual gallery matrix that the render-outlier scorer predicts degraded, so degradation is a checked regression.

#### Scenario: Gallery flags a predicted-bad scenario
- **WHEN** the gallery matrix renders a scenario the scorer predicts degraded
- **THEN** the scenario is marked as a render-outlier in the gallery output and the CI check surfaces it.

#### Scenario: Gallery stays clean for healthy scenarios
- **WHEN** all matrix scenarios are predicted healthy
- **THEN** the gallery output carries no render-outlier flags.