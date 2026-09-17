# Gui Flow Verification Specification

## Purpose

Make the winit event-loop boundary testable and tested, so the keyboard dispatch and window close/quit behavior that regressed this week cannot silently break again.

## Requirements

### Requirement: Window close decision is a tested pure seam

The window-close decision SHALL be expressed as a pure function returning an outcome, covered by a test asserting that a close request maps to close-and-exit.

#### Scenario: Close request exits the loop

- **WHEN** the window receives `WindowEvent::CloseRequested`
- **THEN** the outcome function returns close-and-exit, and a test asserts this without a live event loop

### Requirement: Windowed keyboard seam is regression-tested

`handle_window_key_event` SHALL map `q` to a quit signal and each primary keybinding to its App-state change, covered by tests that synthesize winit key parts.

#### Scenario: q quits

- **WHEN** `handle_window_key_event` receives the `q` key pressed
- **THEN** it returns a quit signal, asserted by a test

#### Scenario: l opens the picker

- **WHEN** `handle_window_key_event` receives the `l` key pressed
- **THEN** the App opens the file picker, asserted by a test

#### Scenario: g v opens the source viewer

- **WHEN** the `g` prefix is armed and `v` follows
- **THEN** the source viewer opens as a right-column slot, asserted by a test

### Requirement: End-to-end quit flow is proven through repaint

The full chain (key event to handler to App state to repaint) SHALL be exercised by at least one egui_kittest test that confirms the quit signal and a queryable surface after repaint.

#### Scenario: Load and quit end to end

- **WHEN** an egui_kittest harness drives key events through the handler and repaints
- **THEN** the repaint reflects the App state and query-by-label resolves the expected widget
