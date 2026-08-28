## ADDED Requirements

### Requirement: Modifier hue derivation

The system SHALL derive a per-modifier hue as a pure function `modifier_hue(token: &str) -> Color` computed as `hash(token) % 16` over the active ANSI-16 palette, deterministic within a run (same token always same hue) and without adding new stored theme tokens or config keys. The `terminal` theme maps the hue to `Reset`/available tones while preserving distinctness from the error red used for topology errors; `mono` keeps the 16 hues pairwise distinguishable via modifiers where needed.

#### Scenario: Pure derivation

- **WHEN** `modifier_hue("B1.1")` is called repeatedly
- **THEN** it returns the same `Color` each time.

#### Scenario: Theme parity

- **WHEN** the `classic`, `terminal`, or `mono` theme is active and a modifier is highlighted
- **THEN** the hue is drawn from that theme's palette and does not hardcode a literal color in rendering code.

#### Scenario: No new config

- **WHEN** the config file is inspected
- **THEN** no `modifier` color key exists; the hue is derived, not configured.
