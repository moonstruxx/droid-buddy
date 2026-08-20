## Context

The project already has a working ratatui + crossterm TUI scaffold with:
- A `Patch` struct with `HwComponent` and `ShiftGroup` types
- Sample data only (no real file parsing)
- Keyboard-only navigation (j/k, Enter, shift keys 1-4)
- A simple shift-group-row layout in `ui.rs`
- No mouse support, no file picker, no .ini parsing

See proposal.md for motivation and specs for requirements.

## Goals / Non-Goals

**Goals:**
- Parse real DROID `.ini` patch files and render their hardware components
- Organize components by physical controller type (P2B8, Faderbank, etc.) in labeled panels
- Enable mouse interaction (click, hover, scroll) alongside existing keyboard navigation
- Visualize shift groups with colored border frames on affected panels
- Work correctly inside Herdr and tmux on macOS and Linux

**Non-Goals:**
- Real-time hardware synchronization with physical DROID device (future)
- Patch editing or creation (view and interact only for now)
- Audio/MIDI processing or playback
- Custom circuit definition syntax (only parse standard DROID `.ini` format)

## Decisions

### Decision 1: Use `ini` crate for parsing (not `serde_ini`)
**Rationale:** DROID `.ini` files use a non-standard format where section names are circuit types (e.g., `[button]`, `[p2b8]`) and values can be expressions (`_ENV1_DECAY_POT * -1 + _DECAY_MIN`). The `ini` crate gives us raw section/key/value access without requiring serde-compatible structure, which is better for our token-extraction approach. `serde_ini` expects a fixed struct shape that doesn't match DROID's variable circuit sections.

**Alternatives considered:**
- `serde_ini`: Rejected — requires predefined struct, doesn't handle variable sections well
- Manual regex parsing: Rejected — `ini` crate handles section/key parsing reliably

### Decision 2: Hardware token extraction via regex on values
**Rationale:** Hardware tokens follow a consistent pattern: `[BLPOIES]\d+\.\d+` (e.g., `B1.1`, `L2.30`, `P1.1`, `O4`, `I1`, `E1.1`, `S1.3`). We extract these from all circuit section values using a single regex, then map the prefix letter to `ComponentKind`. This avoids needing to understand DROID's expression syntax — we just find the tokens.

**Token mapping:**
- `B` → Button
- `L` → Led
- `P` → Knob (Pot)
- `O` → CvOut
- `I` → CvIn
- `E` → Encoder
- `S` → Switch

### Decision 3: Controller panel grouping by token prefix range
**Rationale:** DROID controllers are identified by token number ranges:
- P2B8: B1.1-B1.8, L1.1-L1.8, P1.1-P1.2 (controller 1)
- Faderbank: Fader tokens (O1-O4 typically)
- Notebuttons: B2.1-B2.12, L2.1-L2.12 (controller 2)
- Encoder: E1.1, E2.1, etc.

We group components by their controller number (the first digit after the letter) and known controller types. A `[p2b8]` section explicitly declares a P2B8 controller; `[notebuttons]` declares a Notebuttons controller. For implicit controllers, we infer from token ranges.

### Decision 4: Mouse via crossterm `EnableMouseCapture`
**Rationale:** Crossterm's mouse capture is the standard approach for ratatui apps. It works inside Herdr and tmux when the multiplexer has mouse mode enabled. We handle `Event::Mouse` alongside `Event::Key` in the main event loop.

### Decision 5: Shift visualization via panel border styling
**Rationale:** When a shift key (1-4) is pressed, we identify which controller panels contain components from that shift group and apply a colored, bold border. Unrelated panels get a dim gray border. This is cleaner than background tinting (which clashes with component colors) or label prefixes (which are less visually impactful).

### Decision 6: Keep existing keyboard navigation
**Rationale:** Mouse interaction supplements but does not replace keyboard navigation. All existing key bindings (j/k, Enter/Space, 1-4, Esc, q) are preserved. This ensures the app works in environments where mouse passthrough is unavailable.

## Risks / Trade-offs

| Risk | Mitigation |
|------|-----------|
| Herdr/tmux mouse passthrough may not work in all configurations | Keyboard navigation remains fully functional; mouse is optional enhancement |
| Large patches may not fit in small terminal windows | Panels wrap to multiple rows; terminal resize events trigger reflow |
| DROID `.ini` format may have edge cases not covered by token regex | Start with known circuit types from `droid-circuit-examples.md`; extend regex as needed |
| `ini` crate may not handle all DROID `.ini` variations | Fallback to manual line-by-line parsing if needed; `ini` handles 95% of cases |
| Unicode symbols (●, ○, ◉, ▣, □) may not render in all terminals | These are widely supported in modern terminals; fall back to ASCII (`*`, `o`, `@`, `#`, `[]`) if needed |
