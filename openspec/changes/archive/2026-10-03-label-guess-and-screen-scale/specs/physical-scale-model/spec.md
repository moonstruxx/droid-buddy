# Spec Delta

## Purpose

The physical view renders its cells under one compact contract whose label now lives at its own screen-space size level.

## MODIFIED Requirements

### Requirement: Physical cell rendering contract (compact-only)

The system SHALL render physical-view element cells under a single compact-cell contract: a component cell always draws its state glyph, the label shares the first row at a fixed screen-space font size independent of the physical zoom — ellipsized to the cell width — when the cell is wide enough for at least one character of that font, and SHALL be omitted when the cell is narrower than one character of the label font; the state text takes the second row when the cell is tall enough. The boxed-LED presentation path is removed. An LED that belongs to an element SHALL render inside that element's cell as defined by the `module-ui` capability and SHALL NOT render as a co-located or standalone cell; the element rect equals the compact cell.

#### Scenario: Label size independent of physical zoom

- **WHEN** the user cycles the physical zoom presets from 75 % to 200 %
- **THEN** every rendered cell label uses the same font size in points while the cells scale with the zoom

#### Scenario: LED element renders the compact cell

- **WHEN** a patch declares an LED-associated element
- **THEN** the physical view renders one compact cell for the element with the LED state inside it, and no separate LED cell

#### Scenario: Boxed-LED branch is unreachable

- **WHEN** the physical view renders any element cell at any zoom level
- **THEN** no element cell is drawn through the boxed-LED path

#### Scenario: Narrow cell hides the label

- **WHEN** a cell is narrower than one character of the label font at any zoom preset
- **THEN** the cell draws its state glyph without a label, and the full label remains available on hover and in the status bar
