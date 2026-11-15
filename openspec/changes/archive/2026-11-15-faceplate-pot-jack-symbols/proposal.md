# Proposal

## Why

`module-ui` already requires a pot to render as "a knob with pointer" and a jack as "a jack" and nothing implements either: `paint_cell` (`src/gui/physical.rs:1324`) draws a pot/encoder cell as its fitted label plus a top-right `◉` marker and a `NN%` row, and a CV cell as a label plus `◀`/`▶`. Buttons got real faces in `button-led-composition` (`paint_composed`, `src/gui/physical.rs:1554`), so a faceplate now reads as circles for buttons and bare magenta text for pots — the one element family the module UI claims to draw at 1:1 without drawing.

The two references that fix the symbols must survive the cells the geometry actually produces: 12 mm pots (P2B8, P4B2, P10 big) are 45 pt at 1:1, 9 mm pots (P10 small) 34 pt, master CV jacks 26 pt (7 mm), and at the 37 % rack fit of `fixtures/physical_multirow_rack.ini` those fall to 17 / 13 / 10 pt. Detail therefore has to be a ladder, not one drawing scaled.

## What Changes

- **Pot face (finite rotary).** A knob draws a washer ring enclosing a hex nut, a shaft and a slot, seated on a horizontal base line: the base says the element has end stops.
- **Encoder face (endless rotary).** An encoder draws the same face **without** the base line — the one stroke that still resolves at a 10 pt face, where three legs (the reference icon's) would already be mush.
- **Jack face.** A CV in/out draws a bezel ring enclosing a filled socket mouth with the cross the contacts cut into it.
- **The slot carries the value**, 270° sweep, 0 at 7:30 and 1 at 4:30. The `NN%` state row is not special-cased: the existing label ladder keeps it only when the band still holds it after the label.
- **Level of detail by drawn face diameter**: at or above 18 pt the full symbol; 10–18 pt ring + slot (nut, shaft and base line drop); below 10 pt ring + dot. Pot and encoder converge below 18 pt, exactly as they already do today at the shared `◉`.
- **Every face keeps the cell** and pushes the label into the band beneath it, `h − 0.66 w`, the way a composed button face keeps its shape and labels its band. A face never shrinks or displaces for a label.
- **Unaddressed faces**: an unaddressed rotary drops the washer ring — the ring is what *addressed* adds — keeping the nut, slot and (on a pot) the base line; an unaddressed jack keeps its full face. Both dimmed. This retires the ring-plus-dot rule for rotaries.
- **Direction never rides colour.** The `mono` palette maps `cv_in` and `cv_out` to the same Gray, so the `◀`/`▶` corner marker and the `CV IN`/`CV OUT` state row stay the direction channel; the face carries identity only.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `module-ui`: **Faceplate element fidelity** — the pot, encoder and jack symbols, the LOD ladder, the face depth and label band, and the invariant that a face changes neither the published rect nor the hit or wheel behaviour.
- `module-ui`: **Unused elements are drawn** — an unaddressed rotary renders the same symbol dimmed with the washer ring omitted, an unaddressed jack renders its full face dimmed, and no other element kind gains a face.

## Impact

- `src/physical.rs` — `CellTextPath::Face` and `FACE_HEIGHT_FRACTION`, consumed by `label_text_box` through `cell_label_box`; band unit tests.
- `src/gui/physical.rs` — `CellShape::{Pot, Encoder, Jack}` on `CellSpec`, resolved in `physical_spec` from `ComponentKind` and the geometry family; `paint_pot` and `paint_jack` beside `paint_fader`/`paint_composed`; dispatch in `paint_cell`; the unaddressed branch; `CellSpec::fader_value` renamed `value_frac` because the slot reads the same 0..1 position the fader track does.
- `src/regression.rs` — the labels-fit zoom, the `auto` annotation set and the fit clamp move to the new band; published hit rects stay identical.
- `docs/labels.md`, `docs/views.md`; regenerated `DESIGN.md` / `ARCHITECTURE.md` (`DESIGN.md:217` and `:224` carry the claims this change makes true and retires); regenerated `docs/assets/module-ui.png` and `module-ui-labels.png`.
- No new theme token: `knob`, `cv_in` and `cv_out` colour the faces, and shape carries kind, finiteness and direction.

## Non-goals

- No graph change: jack nodes keep their title plus port dot, which is the graph's own schematic convention; no node icon slot is introduced.
- No body, legs or other artwork from the reference pot icon — the base line replaces the legs because it survives the zoom they do not.
- No new theme token, and no change to the `mono`/`terminal` palettes.
- No change to hit rects, wheel adjust, keyboard navigation, skeleton `PortMark` glyphs, the fader face, composed button faces, LED cells, or the S10 rotary and toggle switch glyphs.
- No per-element or per-controller configuration of the symbols.
