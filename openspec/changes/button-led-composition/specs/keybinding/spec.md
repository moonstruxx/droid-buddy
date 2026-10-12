## ADDED Requirements

### Requirement: LED toggle chord on the module UI

While the module UI pane is focused, `Ctrl+Enter` SHALL toggle the focused element's folded LED without changing the element's own state, and `Enter`/`Space` SHALL continue to toggle the element only. With no folded LED bound to the focused element the chord SHALL be a silent no-op, and while processing is paused it SHALL change nothing. The help modal's Module UI table SHALL list the chord.

#### Scenario: Chord toggles only the LED

- **WHEN** the module UI is focused, an element with a folded LED is hovered, and the user presses `Ctrl+Enter`
- **THEN** the folded LED's state toggles, the element's own state is unchanged, and the status bar reports the LED token and its new state

#### Scenario: Enter does not toggle the LED

- **WHEN** an element with a folded LED is hovered and the user presses `Enter` or `Space`
- **THEN** the element's own state toggles and its folded LED's state is unchanged

#### Scenario: No folded LED is a no-op

- **WHEN** the focused element has no bound LED and the user presses `Ctrl+Enter`
- **THEN** nothing changes and no status message is produced

#### Scenario: Paused blocks the chord

- **WHEN** processing is paused and the user presses `Ctrl+Enter` on an element with a folded LED
- **THEN** the LED's state is unchanged

#### Scenario: Help lists the chord

- **WHEN** the user opens help with `?` from the module UI
- **THEN** the Module UI key table lists `Ctrl+Enter` with its LED-toggle meaning
