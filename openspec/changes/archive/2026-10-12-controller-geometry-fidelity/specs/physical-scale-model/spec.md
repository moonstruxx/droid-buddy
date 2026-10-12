## ADDED Requirements

### Requirement: Controller geometry is measured and verified

The embedded per-controller geometry SHALL carry each element family's real faceplate arrangement and cell extents as measured from the panel — including whether the panel stacks a family in one column or lays it out as a matrix — rather than assumed pitch values, and SHALL be regenerable from its generator and verified: every element cell lies inside its module rect, every declared grid matches its token-family counts, and any grid shape it shares with `rack_geometry.json` agrees.

#### Scenario: Cells lie inside the module

- **WHEN** the embedded controller geometry is verified
- **THEN** every element cell of every controller lies within its module's width and height, and every cell has a positive size

#### Scenario: Grids match token counts

- **WHEN** a controller declares an element grid of a given column and row count
- **THEN** the cells emitted for that family match the declared grid and the controller's token-family count

#### Scenario: Shared grid shapes agree

- **WHEN** the two embedded geometry files both describe a controller's element grid
- **THEN** their grid shapes agree — E4 is a vertical stack of four encoders in both — and a disagreement fails verification
