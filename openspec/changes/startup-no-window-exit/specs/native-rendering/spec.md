# Spec Delta

## MODIFIED Requirements

### Requirement: The native window is the application
The system SHALL run as a single native window (winit + egui + wgpu) with no terminal UI. The window SHALL own the event loop, input, and rendering. There SHALL be no terminal fallback, no headless runtime mode, and no feature flag that restores the terminal. A launch that cannot open its window SHALL end with a non-zero exit status and a diagnostic naming the cause; the process SHALL NOT exit successfully without a window having been opened at any point.

#### Scenario: Launch opens a window
- **WHEN** the user launches the app
- **THEN** a native window opens and owns the event loop and rendering

#### Scenario: No terminal fallback
- **WHEN** the app runs in an environment that cannot open a native window
- **THEN** the app does not fall back to a terminal UI; it reports a clear error

#### Scenario: An unopenable window fails loudly
- **WHEN** the event loop exists but the startup window cannot be created
- **THEN** the open failure is reported as the process's error and the exit status is non-zero, instead of the app continuing windowless or exiting successfully with only a warning on stderr

#### Scenario: No window is never a success
- **WHEN** the event loop returns without a window ever having been opened
- **THEN** the run ends with an error stating that the window never opened, and the exit status is non-zero

## ADDED Requirements

### Requirement: Panic teardown is total

The panic path SHALL tear the window and its canvas down without ever panicking itself: teardown SHALL be idempotent, so repeated calls are no-ops, and every thread-local access it performs SHALL tolerate a value that is unavailable, already taken, or already destroyed by thread-exit. A panic that occurs while the process is already unwinding — including one raised by dropping a GPU object — SHALL NOT abort the process through a second panic, and the panic hook SHALL still chain to the previously installed hook.

#### Scenario: Teardown runs twice
- **WHEN** the panic teardown runs and is called again
- **THEN** the second call returns without touching anything and without panicking

#### Scenario: A failing teardown step is swallowed
- **WHEN** a teardown step panics (for example because a thread-local was already destroyed)
- **THEN** the panic is contained, the remaining teardown steps still run, and the process continues unwinding through the normal hook instead of aborting

#### Scenario: Unwinding through the hook reaches the previous hook
- **WHEN** a panic occurs while the window is open
- **THEN** the window is torn down and the previously installed hook still receives the panic information
