# theming Specification

## Purpose
A semantic color-token engine that lets the whole TUI restyle itself from one named palette instead of scattered hardcoded colors.

## Requirements

### Requirement: Semantic color tokens
The UI SHALL derive every color from a single set of named semantic tokens: one per component kind (button, knob, cv-in, cv-out, led), one per shift group (1–4), plus accent, muted, text, viewer-key/hint, and status-background tokens. No rendering code outside the token layer SHALL hardcode colors.

#### Scenario: Token coverage
- **WHEN** any panel, component, shift highlight, status bar, picker, or source-viewer element is rendered
- **THEN** its colors come only from the active theme's tokens

### Requirement: Built-in themes
The system SHALL ship three compiled-in ANSI-16 themes selectable by name: `classic` (the default; preserves today's exact color mapping), `terminal` (all tokens resolve to the terminal's own default/reset colors), and `mono` (grayscale plus a single accent).

#### Scenario: Default unchanged
- **WHEN** no config exists and the app starts
- **THEN** the `classic` theme is active and rendering is indistinguishable from before this change

#### Scenario: Terminal theme
- **WHEN** `theme = "terminal"` is selected
- **THEN** foreground/background colors defer to the host terminal scheme (reset colors), with emphasis expressed via modifiers rather than hues

### Requirement: Shift-group distinctness per theme
Every shipped theme SHALL assign mutually distinct colors to the four shift-group tokens so groups 1–4 remain visually separable.

#### Scenario: Distinctness holds everywhere
- **WHEN** any built-in theme is inspected
- **THEN** its four shift-group token values are pairwise different

### Requirement: Canonical theme name resolution
Theme names in configuration SHALL match case-insensitively with `-`, `_`, and space treated as equivalent; an unrecognized name falls back to the default theme with a one-time stderr warning listing valid names.

#### Scenario: Name normalization
- **WHEN** the config specifies `theme = "TOKYO_NIGHT"` for a catalog entry named `tokyo-night` (illustrative)
- **THEN** that theme is selected

#### Scenario: Fallback
- **WHEN** an unrecognized name is given
- **THEN** the default theme applies and the user sees which names are valid

### Requirement: Switch token across palettes

The theme layer SHALL define a `switch` semantic token used by Switch cells, with values: `classic` = white (unchanged from the previous button color), `terminal` = `Reset`, `mono` = a gray distinct from the button token's gray (e.g. dark-gray).

#### Scenario: Token resolves per palette

- **WHEN** a Switch renders under `classic`, `terminal`, or `mono`
- **THEN** it uses that palette's `switch` token value.

#### Scenario: Classic byte-identical

- **WHEN** the `classic` palette renders a Switch
- **THEN** the rendered color is byte-identical to the pre-change color.

### Requirement: Display placeholder token

The theme layer SHALL define a `display_placeholder` semantic token used for the DB8E OLED display placeholder's border and centered text, with values in every built-in palette (`classic`, `terminal`, `mono`) and applied through `theme::active()` with no hardcoded `Color::` literal in the renderer.

#### Scenario: Token resolves per palette

- **WHEN** the DB8E placeholder renders under `classic`, `terminal`, or `mono`
- **THEN** its border and text use that palette's `display_placeholder` token value.

#### Scenario: Classic preserves existing palette intent

- **WHEN** the `classic` palette is active
- **THEN** the `display_placeholder` token resolves to a muted/neutral color distinct from the fader amber, LED red, and accent blue so the OLED frame reads as a display surface.

### Requirement: Cluster-title contrast floor

Every shipped palette SHALL resolve the signal-flow graph's cluster-title token to a foreground that meets a 4.5:1 contrast ratio against that palette's graph canvas background, so a banner-group name is readable wherever its enclosing rectangle is drawn. Where a palette resolves both the title and the canvas background to the terminal default (`Reset`), no RGB pair exists and the palette defers to the host scheme instead of meeting the floor.

The `classic` palette's cluster title SHALL be a blue that is legible on the black canvas background (ANSI LightBlue), not ANSI Blue, and the palette's cluster border SHALL remain ANSI Blue.

#### Scenario: Classic cluster titles are legible

- **WHEN** the `classic` palette is active and the graph paints a cluster title
- **THEN** the title resolves to ANSI LightBlue (`0x80,0x80,0xff`) and its contrast ratio against the classic canvas background (black) is at least 4.5:1

#### Scenario: Every palette clears the floor or defers

- **WHEN** each shipped palette is inspected
- **THEN** `classic` and `mono` each resolve a cluster title that clears 4.5:1 against their canvas background, and `terminal` resolves both tokens to `Reset` and is exempt
