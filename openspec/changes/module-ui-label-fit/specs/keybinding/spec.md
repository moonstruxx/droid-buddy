# Spec Delta

## MODIFIED Requirements

### Requirement: Physical-view navigation keys

The system SHALL provide keys to pan the physical view when the rack overflows the pane, to change zoom, to fit the rack to the pane, and to toggle the skeleton reference mode. Overflow SHALL be judged against the pane the module UI is actually drawn in, not against a stored viewport size, so panning works whenever the drawn rack exceeds the pane on the pressed axis. The system SHALL provide two fit keys while the module UI holds focus: `f` SHALL apply the label fit (the larger of the whole-rack fit and the zoom every written label needs, clamped to the supported range) and `Shift+F` SHALL apply the rack fit (the whole-rack fit alone). The status report SHALL name which fit applied.

#### Scenario: Pan keys move the viewport

- **WHEN** the rack is larger than the pane and the user presses a pan key
- **THEN** the viewport offset moves in the corresponding direction without changing zoom, and the status reports the offset

#### Scenario: Pan keys are inert on a fitting rack

- **WHEN** the drawn rack fits the pane on an axis and the user presses the pan key for that axis
- **THEN** the view does not move and the key falls through to its navigation meaning

#### Scenario: Fit key applies the label fit

- **WHEN** the user has zoomed and panned and then presses `f`
- **THEN** the view applies the label fit — every written label fits its cell — and the status reports `fit(labels)`

#### Scenario: Fit key returns to the fitted view

- **WHEN** the user has zoomed and panned and then presses the fit key
- **THEN** the rack is scaled to that fit and centred, and the status reports which fit applied

#### Scenario: Shift+F applies the rack fit

- **WHEN** the user presses `Shift+F`
- **THEN** the view fits the whole rack to the pane, centring it, and the status reports `fit(rack)`

#### Scenario: Skeleton toggle switches presentation

- **WHEN** the user presses the skeleton-toggle key
- **THEN** the main view switches between full and skeleton presentation of the same layout, and back.
