# patch-parsing Specification Delta

## MODIFIED Requirements

### Requirement: Record LED association

The system SHALL link a hardware LED token to the element defined in the same section, exposing the association on the parsed patch's component as an optional LED reference. The association is **device-dependent**: an explicit `led = L.N` entry and a numbered `ledN = L.M` entry paired by shared numeric suffix with a same-suffix element entry (`buttonN`/`potN`/`encoderN`/`switchN`/`faderN`) remain authoritative; when a section has no explicit pairing, the parser applies the controller/device's default LED wiring — M4 fader touch plates are RGB (`L` + `R` registers per fader), B32 buttons are white-only, and master LEDs default-link to their CD channels.

#### Scenario: Button with LED

- **WHEN** a `[button]` section contains `b = B1.1` and `led = L1.1`
- **THEN** the parsed button component carries the association `led: Some("L1.1")` alongside its id `B1.1`.

#### Scenario: Section without led

- **WHEN** a section defines an element but no `led =` assignment
- **THEN** the parsed component has no explicit LED association (`led: None`) unless the device default applies.

#### Scenario: Existing parse unchanged

- **WHEN** a patch contains no `led =` assignments at all
- **THEN** every component parses with `led: None` — no behavioral change.

#### Scenario: M4 fader touch plate resolves its RGB LED

- **WHEN** a patch declares an M4 fader touch plate (B-family token) with no explicit `led =` assignment
- **THEN** the parser associates the device-default RGB LED (`L`/`R` pair) for that touch plate, so the physical view can render its LED state.

#### Scenario: B32 button stays white-only

- **WHEN** a patch declares a B32 button with no explicit `led =` assignment
- **THEN** the parser does not associate an RGB LED; the button renders white-only per the device default.

#### Scenario: Explicit pairing overrides the device default

- **WHEN** a section carries an explicit `ledN = L.M` paired with a same-suffix element
- **THEN** that explicit LED association is authoritative and the device default is not applied for that element.
