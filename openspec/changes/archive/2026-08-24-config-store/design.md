# Design: config-store

## Context

See proposal.md — Why. Current state: 103 `Color::` literals in `ui.rs`, 4 in `patch.rs` (via `ShiftGroup::color()`); no config file; `serde` already a dependency; main.rs loads nothing before `ratatui::init()`. The herdr project (`~/projects/ai_ls/herdr`) demonstrates the pattern this change adapts: one token struct (`Palette`), compiled-in palette constructors, canonical name matching, per-token overrides via a small color parser.

## Goals / Non-Goals

Goals:
- One source of truth for every UI color; theming becomes data.
- Zero behavior change by default; opt-in via one TOML key.
- Config write path exists from day one so later preference features are drop-in.

Non-Goals (design-level additions to the proposal's list):
- No hot-reload of the config file while running.
- No migration/versioning machinery for the config schema — v1 has one key; unknown keys are ignored forward-compatibly instead.

## Decisions

### Decision 1: Token struct mirrors DESIGN.md role families (~14 fields), not herdr's 19-token Palette
herdr's tokens (panel_bg, sidebar_bg, active_row_bg…) describe an agent-list app. Ours describe a patch viewer. Consumers: `ui.rs` render functions and `patch.rs` shift colors. Fewer, semantically-native tokens keep every field consumed (YAGNI rule).
*Alternative considered*: adopt herdr's exact Palette shape — rejected; several fields would have no consumer.

### Decision 2: Palettes compiled-in as Rust constructors; `terminal` theme uses `Color::Reset`
No file I/O for built-ins, no serde needed for themes themselves. `classic` is byte-for-byte today's mapping so "no config" equals "no diff". `mono` gives a screenshot-friendly neutral option.
*Alternative considered*: embedding herdr's 18 truecolor palettes — rejected by user decision B; ANSI-16 design decision stands.

### Decision 3: `ShiftGroup::color()` retires into the token layer
Today two sources of truth would exist (theme + enum method). The enum keeps `key_label()`; color lookup moves to theme tokens keyed by group index.
*Alternative considered*: delegate `color()` to the theme — rejected; keeps a color-shaped API alive that invites bypassing the token layer.

### Decision 4: TOML via the maintained `toml` crate; YAML explicitly declined
`serde_yaml` is archived; forks add supply-chain risk for zero benefit. TOML is herdr-proven and Rust-native.
*Alternative considered*: `serde-yaml-ng` — rejected after user accepted TOML.

### Decision 5: Load once at startup in main.rs before `ratatui::init()`; warn-once on stderr
The app is single-threaded and event-driven; re-reading config mid-session buys nothing today. Warnings go to stderr *before* alternate-screen entry so users can actually read them.
*Alternative considered*: runtime reload keybind — YAGNI, no consumer.

### Decision 6: Write path = temp file + rename in target dir
Atomic on POSIX, avoids partial configs if the process dies mid-write. Directory creation on demand. Unused by production code in v1 (tests exercise it); rack prefs will be first real caller.

## Risks / Trade-offs

- **Re-pointing ~103 color sites is mechanical but broad** → mitigated by doing it in one wave with clippy `-D warnings` + full test suite as gates; ui tests assert via tokens, not literals, so they survive future theme edits.
- **ANSI-16 limits expression** → accepted trade-off (Decision 2); truecolor remains possible later without schema change since tokens are typed `ratatui::style::Color`.
- **Stale spec debt discovered**: `openspec/specs/keybinding/spec.md` carries unimplemented "Resize mode" requirements. Out of scope here — flagged for separate cleanup.
