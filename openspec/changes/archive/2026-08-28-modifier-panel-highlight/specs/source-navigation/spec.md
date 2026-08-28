## ADDED Requirements

### Requirement: Per-modifier hue for affected spans

When a modifier is active (momentary hold or latched), the system SHALL recolor every `select` line span that is in that modifier's structural influence (i.e., the `ModifierAffect.span` set for the token, unioned across additive latches) with that modifier's stable hue, rather than a single generic highlight. When multiple modifiers are latched, spans keep the hue of their source token (most-recent wins on overlap). Clearing the modifier SHALL remove all span tints. Structural-only scope (no `selectat` gating) matches the panel graph walk.

#### Scenario: Span hue matches panel hue

- **WHEN** `B1.1` (cyan) is held
- **THEN** every `select = B1.1` or `select = _VAR` derived from `B1.1` line highlights in cyan, the same hue as the panel cell wash.

#### Scenario: Additive hues

- **WHEN** `B1.1` and `B1.2` are latched
- **THEN** spans from each retain their respective hues.

#### Scenario: Cleared

- **WHEN** `Esc` clears the modifier
- **THEN** no source spans remain tinted.
