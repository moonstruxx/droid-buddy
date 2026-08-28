## ADDED Requirements

### Requirement: Switch token across palettes

The theme layer SHALL define a `switch` semantic token used by Switch cells, with values: `classic` = white (unchanged from the previous button color), `terminal` = `Reset`, `mono` = a gray distinct from the button token's gray (e.g. dark-gray).

#### Scenario: Token resolves per palette

- **WHEN** a Switch renders under `classic`, `terminal`, or `mono`
- **THEN** it uses that palette's `switch` token value.

#### Scenario: Classic byte-identical

- **WHEN** the `classic` palette renders a Switch
- **THEN** the rendered color is byte-identical to the pre-change color.