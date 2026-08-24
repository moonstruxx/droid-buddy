# controller-panels Specification (delta)

## ADDED Requirements

### Box LED-associated elements

An element with an associated LED (`led: Some(...)`) SHALL render as a single bordered cell:
- Border color: the element's kind color (button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red).
- Inside the border: element symbol, label, state, and LED glyph reflecting the LED component's state.
- The LED component SHALL NOT render as its own standalone cell.

**Scenarios:**

- **P2B8 button with LED**: B1.1 renders as a bordered box with white border, showing the button's symbol/label/state and the LED's glyph/state inside.
- **Knob without LED**: A pot renders as today's two-line text cell (no border, no box).

### Box geometry and hit-testing

Boxed cells use the updated component cell geometry (height 3). The `component_rects` vector SHALL reflect the boxed cell for click hit-testing.

**Scenarios:**

- **Click on boxed cell**: Clicking anywhere inside the bordered box toggles/selects the element.
- **LED state changes**: When the LED state changes, the glyph inside the box updates accordingly.
