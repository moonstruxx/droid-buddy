# visual-validation Specification

## Purpose

Provide an inspectable visual face for the terminal UI, compare it to spec intent, fail the build on face regression, and preserve the face as durable proof in the change archive — so UI evolution is tracked.

## Requirements

### Requirement: Egui shape and label assertions
The system SHALL verify each surface's rendering by driving a headless egui context and asserting on the produced shapes and labels (element cells, borders, titles, state glyphs), without opening a window or GPU.

#### Scenario: Surface render is asserted headlessly
- **WHEN** a surface renders into a headless egui context
- **THEN** a test asserts the expected shapes and labels appear in the output
