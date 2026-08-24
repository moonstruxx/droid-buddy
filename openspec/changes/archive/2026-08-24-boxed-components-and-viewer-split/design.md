# Design: boxed-components-and-viewer-split

## Context

The droid_tui component grid currently renders every hardware element as a plain two-line text cell. DROID patches wire LEDs to their owning elements via `led = L.N` in the same `.ini` section (e.g. `[button]` with `b = B1.1` and `led = L1.1`). This pairing is invisible in the UI — the button and LED appear as two separate unconnected cells.

Additionally, the embedded source viewer uses a fixed 50/50 split that halves the panel area when open.

## Goals / Non-Goals

**Goals:**
- Visually unite elements with their associated LEDs into single boxed cells.
- Make the source viewer split adjustable and panels-favoring by default.
- Preserve backward compatibility for elements without LEDs.

**Non-Goals:**
- LED RGB/color detection (future work).
- Mouse-drag split resize (keyboard-only).
- Schema validation against `circuits.json`.
- Persistence of split ratio across sessions.

## Decisions

### D1: LED association captured at parse time

The parser records, per section, which element owns which LED. When a section assigns `led = L.N`, the parser links that LED token to the element defined in the same section (from `b =`, `e =`, `fader =`, etc.). This association is exposed on `HwComponent` as `led: Option<String>`.

**Rationale:** The wiring is explicit in the `.ini` file — no id-convention guessing needed.

### D2: Fold LED into owner box

An element with an associated LED renders as ONE boxed cell. The LED component is **not** rendered as its own standalone cell — it is folded into the owner's box. The box displays the element's symbol, label, state, and the LED's glyph/state.

Elements without an associated LED keep today's two-line text rendering (no border, no box).

**Rationale:** The user's core complaint was "two text fields" for a button+LED pair. One box per pair resolves this.

### D3: Unified cell height of 3

Boxed cells need 3 rows (top border + content + bottom border). To keep grid math simple with a mixed grid (some boxed, some text), all cells use height 3. Text cells render their 2 content lines with a blank line padding.

COMPONENT_WIDTH stays at 16 (or grows to 18 for wider boxes). COMPONENT_HEIGHT changes from 2 to 3.

**Rationale:** Uniform cell height keeps the grid layout arithmetic simple. Mixed per-cell heights would require per-row height tracking.

### D4: Adjustable viewer split ratio

`App` gains `viewer_split_ratio: f32` (default 0.6 = 60% panels / 40% source). Keys `[` and `]` adjust ±0.1, clamped to 0.3–0.7. The split is applied in `render_embedded_main`.

**Rationale:** The source pane should not block the panels. 60/40 default keeps panels dominant. Clamp prevents unusable extremes.

### D5: Status hints update

The viewer status bar mentions `[` / `]` for split adjustment. The main status bar shows split ratio when the viewer is open.

## Risks / Trade-offs

- **Mixed grid complexity:** Boxed and text cells coexist in the same grid. Mitigated by uniform cell height.
- **LED association accuracy:** Parser relies on `led =` being in the same section as the element. If a patch omits `led =`, the element renders without a box (graceful degradation).
- **Split ratio persistence:** Not persisted — resets to 0.6 on each session. Acceptable for a viewer tool.

## Migration Plan

1. Parser change is backward-compatible: existing patches without `led =` simply produce `led: None` on all components.
2. Boxed rendering is additive: elements without LEDs render exactly as before.
3. Split ratio defaults to panels-favoring; no migration of user preferences needed.

## Open Questions

- Should the box border color use the element's kind color or a neutral color? Proposal uses kind color (button=white, knob=magenta, etc.) for visual consistency with the existing color system.
- Should `COMPONENT_WIDTH` grow from 16 to 18 to accommodate the box border? Keep at 16 initially; widen if content overflows.
