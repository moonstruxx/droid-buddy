# Proposal: config-store

## Why

All 107 color decisions are hardcoded literals scattered across `ui.rs`/`patch.rs`, and the app persists no user preferences between sessions. Users cannot adjust the look of the TUI to their terminal scheme, and upcoming view preferences have nowhere to live. Stealing the proven minimal pattern from the herdr project (single token struct + compiled-in palettes + tiny config file) gives us both a theme engine and a durable config home in one small change.

## What Changes

- Add `src/theme.rs`: a `Theme` struct of ~14 semantic ANSI-16 color tokens covering every role DESIGN.md documents (component kinds button/knob/cv-in/cv-out/led, shift groups 1–4, accents, muted, text, viewer key/hint, status background).
- Ship three compiled-in themes: `classic` (default — byte-for-byte today's look), `terminal` (`Color::Reset` everywhere), `mono` (grays + one accent). Canonical name matching with graceful fallback (herdr pattern).
- Re-point all hardcoded colors in `ui.rs` and `patch.rs` (`ShiftGroup::color()`) at the active theme's tokens; retire `ShiftGroup::color()` as a second source of truth.
- Add per-theme test that shift-group tokens stay mutually distinct.
- Add `src/config.rs`: TOML config file at `$XDG_CONFIG_HOME/droid-tui/config.toml` (default `~/.config/droid-tui/`). Schema v1: single global key `theme = "classic"`. Loader runs before `ratatui::init()`; missing dir/file is silent; malformed file warns on stderr once and falls back to defaults; unknown theme name warns and falls back to `classic`.
- Include a write path in the config API from day one (atomic tmp-file + rename) — currently unused by any caller except tests; rack-view preferences will consume it later.
- New dependency: `toml` crate (+ existing `serde`).

## Capabilities

**New Capabilities:**
- `configuration` — discovery/loading/validation/writing of the user config file.
- `theming` — semantic color-token engine, built-in themes, selection resolution, per-theme distinctness guarantees.

**Modified Capabilities:**
- `controller-panels` — the boxed-cell requirement enumerates concrete kind colors (button=white, knob=magenta, cv-in=cyan, cv-out=green, led=red); those become "the kind colors of the active theme" with `classic` preserving today's mapping.
- `shift-visualization` — group colors are pinned (Group1=Yellow, Group2=Cyan, Group3=Magenta, Group4=Green); they become theme tokens with `classic` preserving today's mapping and every theme guaranteeing mutual distinctness.

## Impact

- **Code**: new `src/theme.rs`, `src/config.rs`; edits to `src/ui.rs` (re-point ~103 color sites), `src/patch.rs` (`ShiftGroup::color()` retirement), `src/main.rs` (load config before terminal init), `Cargo.toml` (+ `toml`).
- **Docs**: DESIGN.md regenerates with token references instead of literal colors (generated artifact — via `/make-design`, not hand-edited).
- **Specs**: two new capability specs, two modified deltas.
- **Users**: zero behavior change by default (`classic` + absent config file); opt-in theming via one TOML key.

## Non-goals

- No per-patch `[racks]` section — module order / region split preferences await layout features that don't exist yet (explicitly deferred).
- No per-token color overrides (`[theme.custom]`) — revisit only when a real consumer appears.
- No CLI flag or env var for theme selection; config file is the single selection surface.
- No truecolor palettes; the flat ANSI-16 design decision stands.
- No light/dark auto-switching (herdr's OSC machinery stays un-stolen).
