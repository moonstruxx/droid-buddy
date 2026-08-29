# Controller Panels Specification

## Purpose

Render hardware components grouped by physical controller type in labeled panels that mirror the physical hardware layout. With the physical scale model, the main view renders components at real module sizes, cross-controller gaps, and faceplate element positions instead of wrapped logical panel rows; multi-circuit panels render as side-by-side faceplates.

## MODIFIED Requirements

### Requirement: Position components in physical layout order

The system SHALL position components by the physical grid model — real millimeter cells in chain order (modules sized from HP, elements at their faceplate positions) — rather than by wrapped panel rows. Repeated instances of the same controller render as separate side-by-side faceplates.

#### Scenario: P2B8 button order

- **WHEN** P2B8 buttons are rendered
- **THEN** B1.1 appears first (left), B1.8 appears last (right), in physical order within their module

#### Scenario: P2B8 components at physical cells

- **WHEN** a patch contains tokens B1.1-B1.8, L1.1-L1.8, P1.1-P1.2
- **THEN** the components are rendered at the P2B8 module's real faceplate cells (width in HP, element pitch from the geometry data), not in a uniform wrapped grid.

#### Scenario: Two instances render as two faceplates

- **WHEN** a patch declares two `[p2b8]` sections
- **THEN** two P2B8 faceplates render side by side at their real widths, in declaration order.

### Requirement: Handle overflow with scrolling or wrapping

The system SHALL handle racks wider or taller than the terminal main area by panning/scrolling (the physical view), keeping the 1:1 mapping intact.

#### Scenario: Panel overflow

- **WHEN** a controller panel has more components than fit in the visible area
- **THEN** the view pans/scrolls to reveal the rest, keeping module geometry intact.

#### Scenario: Panel overflow with modules

- **WHEN** a controller chain has more modules than fit in the visible width
- **THEN** the view pans horizontally, maintaining each module's internal component layout.

#### Scenario: Rack wider than terminal pans

- **WHEN** the rack's total width exceeds the terminal main area
- **THEN** the view pans horizontally to reveal the rest, without compressing module geometry.

### Requirement: Box geometry and hit-testing

The system SHALL publish `component_rects` matching the rendered physical cells under the current scale and pan offset, preserving the renderer-owns-geometry contract.

#### Scenario: Click on boxed cell

- **WHEN** the user clicks anywhere inside a bordered box
- **THEN** the element toggles/selects

#### Scenario: LED state changes

- **WHEN** the LED state of a boxed element changes
- **THEN** the glyph inside the box updates accordingly

#### Scenario: Hit rects match rendered cells

- **WHEN** a component renders at a physical cell
- **THEN** its published `component_rects` entry covers exactly the rendered cell's screen area.