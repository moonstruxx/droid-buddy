# theming Specification Delta

## MODIFIED Requirements

### Requirement: Display placeholder token

The theme layer SHALL define a `display_placeholder` semantic token used for the DB8E OLED display placeholder's border and centered text, with values in every built-in palette (`classic`, `terminal`, `mono`) and applied through `theme::active()` with no hardcoded `Color::` literal in the renderer.

#### Scenario: Token resolves per palette

- **WHEN** the DB8E placeholder renders under `classic`, `terminal`, or `mono`
- **THEN** its border and text use that palette's `display_placeholder` token value.

#### Scenario: Classic preserves existing palette intent

- **WHEN** the `classic` palette is active
- **THEN** the `display_placeholder` token resolves to a muted/neutral color distinct from the fader amber, LED red, and accent blue so the OLED frame reads as a display surface.
