# patch-parsing Specification (delta)

## ADDED Requirements

### Record LED association

When a section assigns `led = L.N`, the parser SHALL link that LED token to the element defined in the same section (e.g. `b = B1.1`), exposing the association on the parsed patch's `HwComponent` as `led: Option<String>`.

**Scenarios:**

- **Button with LED**: A `[button]` section with `b = B1.1` and `led = L1.1` produces an `HwComponent` with `id: "B1.1"` and `led: Some("L1.1")`.
- **Section without led**: A `[pot]` section without a `led =` assignment produces an `HwComponent` with `led: None`.
- **Existing parse unchanged**: Patches without any `led =` assignments produce `led: None` on all components — no behavioral change.
