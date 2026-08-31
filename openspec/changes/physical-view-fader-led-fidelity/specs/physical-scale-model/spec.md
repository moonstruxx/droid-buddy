# physical-scale-model Specification Delta

## MODIFIED Requirements

### Requirement: Element state rendering

The system SHALL render each element's state on its physical-view cell: buttons/switches show their toggle glyph, knobs/encoders show their percentage, faders show a vertical track proportional to value with an amber LED bar, and CV I/O shows its direction — mirroring the panel view's state rendering, so the physical view is a faithful 1:1 representation of the patch's live state.

#### Scenario: Switch state renders on the physical view

- **WHEN** a patch declares a switch (S-family token) on a controller faceplate
- **THEN** the physical view renders a switch cell showing its current state (on/off glyph), and the cell is hit-testable like other components.

#### Scenario: Knob and encoder state renders on the physical view

- **WHEN** a patch declares a knob or encoder on a controller faceplate
- **THEN** the physical view renders its cell with the current value percentage.

#### Scenario: Fader state renders as a vertical track

- **WHEN** a patch declares a fader (F-family, P register) on an M4 or P8S8 controller
- **THEN** the physical view renders its cell as a vertical track proportional to value, with an amber LED bar mirroring position, and the cell is hit-testable.

#### Scenario: Button state renders on the physical view

- **WHEN** a patch declares a button on a controller faceplate
- **THEN** the physical view renders its cell with the current on/off state.

### Requirement: Physical cell rendering contract (compact-only)

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell always draws its state glyph, the label shares the first row when the cell is wide enough (ellipsized), and the state text takes the second row when the cell is tall enough. The boxed-LED presentation path gated on `width>=5 && height>=3` is removed; LED-associated elements render the compact cell with the LED as a co-located cell, and the element rect equals the compact cell.

#### Scenario: LED element renders the compact cell

- **WHEN** a patch declares an LED-associated element
- **THEN** the physical view renders the compact cell (glyph + truncated state) with the LED as a co-located cell, and never the boxed-LED branch (which requires a cell of width ≥ 5 and height ≥ 3 that no geometry cell reaches at any zoom preset).

#### Scenario: Boxed-LED branch is unreachable

- **WHEN** the physical view renders any element cell at zoom 75%, 100%, 150%, or 200%
- **THEN** no element cell is drawn through the boxed-LED path; every LED element uses the compact contract.
