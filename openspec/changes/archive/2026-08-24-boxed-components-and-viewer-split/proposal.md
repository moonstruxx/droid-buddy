# Proposal: boxed-components-and-viewer-split

## Why

Every hardware element renders as a plain two-line text cell (symbol+label / state). A P2B8 button and its LED — two components wired via `led = L1.N` in the same `[button]` section — appear as two unconnected text cells. Elements that carry an LED deserve a single visual unit: the element boxed, LED state inside, border in the element's kind color.

Separately, opening the source viewer forces a fixed 50/50 split that squeezes the hardware panels to half width; the source window should not block the main window.

## What Changes

- Parser records each section's `led = <token>` assignment, linking the section's element (e.g. `b = B1.1`) to its LED component (`L1.1`).
- Elements with an associated LED render as **one boxed cell**: kind-colored border (button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red), symbol + label + state inside, LED glyph reflecting the LED's state. The LED component no longer renders as its own cell.
- Elements without an LED keep today's two-line text rendering. Cell height unifies at 3 (border needs content space); grid math, scale factor, and `component_rects` hit-testing updated.
- Panels | source split ratio becomes adjustable (`[` / `]`, ±10%, clamped 30–70%), default favoring panels — the viewer no longer blocks the main window.

## Capabilities

### Modified

- `patch-parsing`: parser records each section's `led = <token>` assignment as an association from the section's element to its LED component.
- `controller-panels`: LED-associated elements render as one boxed cell with kind-colored border and LED state inside; non-LED elements keep the two-line text rendering; geometry/hit-testing updated.
- `viewer-layout`: panels|source split ratio is adjustable and defaults in favor of the panels so the source pane does not block the main window.
- `keybinding`: new keys adjust the split ratio; status hints updated.

## Impact

- src/patch.rs: led-association capture + fixture
- src/app.rs: viewer_split_ratio
- src/ui.rs: boxed rendering + ratio-aware split
- src/handler.rs: ratio keys
- Docs (derived): regenerate
- Specs: deltas for 4 capabilities

## Non-goals

- No LED color/RGB capability detection (LED renders in existing red/dim state colors; RGB config is future work).
- No mouse-drag split resize — keyboard-only (YAGNI).
- No change to elements without LEDs.
- No persistence of split ratio across sessions.
- No schema validation against `circuits.json`.
