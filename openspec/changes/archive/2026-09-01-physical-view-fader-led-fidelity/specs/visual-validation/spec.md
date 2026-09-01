# visual-validation Specification Delta

## MODIFIED Requirements

### Requirement: Start-small coverage matrix

The system SHALL cover at least the following scenarios; the matrix is the minimum, not an exhaustive combinatoric expansion.

#### Scenario: Controller-panels face covered

- **WHEN** fixtures `arpeggio1.ini` (P2B8 8 buttons + 2 knobs) and `led_pairs.ini` (mixed boxed/text) are rendered at widths 80 and 120 under each theme
- **THEN** snapshots exist for P2B8 panel, boxed LED border with kind color, and plain text cell distinction.

#### Scenario: Viewer-layout and shift face covered

- **WHEN** `source_navigation.ini` is rendered with the embedded source viewer open vs closed, and with `shift1` active (bold colored border + `SHIFT 1 ACTIVE` chip) at width 100
- **THEN** snapshots exist for viewer open/closed and shift-active states.

#### Scenario: Fader-column face covered

- **WHEN** a fader-controller fixture (P8S8 Faderbank or M4 Motorfader) is rendered at widths 80 and 120 under each theme
- **THEN** snapshots exist pinning the vertical fader track with its amber LED bar at multiple value levels (e.g. 0%, 50%, 100%), proving position mirrors value.

#### Scenario: LED-association face covered

- **WHEN** a device-LED fixture (M4 RGB touch plates, B32 white-only, master LED) is rendered under each theme
- **THEN** snapshots exist proving each device's LED association renders its correct state and color (M4 RGB, B32 white-only, master→CD channel).

#### Scenario: Adjoined-cell overlap-free across zoom presets

- **WHEN** a physical-view fixture renders at zoom 75%, 100%, 150%, and 200%
- **THEN** a strict no-overlap assertion over all published `component_rects` passes at each preset, and the rendered face is snapshotted per preset.
