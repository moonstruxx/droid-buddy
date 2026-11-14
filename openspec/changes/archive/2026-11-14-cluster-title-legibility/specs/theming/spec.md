# Spec Delta

## ADDED Requirements

### Requirement: Cluster-title contrast floor

Every shipped palette SHALL resolve the signal-flow graph's cluster-title token to a foreground that meets a 4.5:1 contrast ratio against that palette's graph canvas background, so a banner-group name is readable wherever its enclosing rectangle is drawn. Where a palette resolves both the title and the canvas background to the terminal default (`Reset`), no RGB pair exists and the palette defers to the host scheme instead of meeting the floor.

The `classic` palette's cluster title SHALL be a blue that is legible on the black canvas background (ANSI LightBlue), not ANSI Blue, and the palette's cluster border SHALL remain ANSI Blue.

#### Scenario: Classic cluster titles are legible

- **WHEN** the `classic` palette is active and the graph paints a cluster title
- **THEN** the title resolves to ANSI LightBlue (`0x80,0x80,0xff`) and its contrast ratio against the classic canvas background (black) is at least 4.5:1

#### Scenario: Every palette clears the floor or defers

- **WHEN** each shipped palette is inspected
- **THEN** `classic` and `mono` each resolve a cluster title that clears 4.5:1 against their canvas background, and `terminal` resolves both tokens to `Reset` and is exempt
