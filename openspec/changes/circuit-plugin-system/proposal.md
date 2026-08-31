# Circuit Plugin System

## Why

The authoritative DROID circuit schema (`ext/droid-lsp/droid-lsp/src/circuits.json`) is compiled into the binary via `include_str!`, so a circuit that DROID ships after the vendored submodule snapshot cannot be used without editing a submodule and rebuilding. Worse, an unknown circuit silently **disables the entire `ram_overflow` check** (`validation.rs::ram_not_checked_when_unknown_circuit`) and degrades `latency::circuit_avg` to `1.0` — so today a patch using a new circuit loses RAM validation altogether. This change lets users add circuit definitions via TOML files without touching the submodule.

## What Changes

- **New plugin loader** (`src/plugin.rs`): discovers `*.toml` files under `$XDG_CONFIG_HOME/droid-tui/plugins/`, parses each, validates per-file (circuit name, category, `ramsize` — required — inputs/outputs with `prefix`/`count`/`start_at`), and merges them over the embedded schema.
- **Merge semantics**: a plugin circuit overrides an embedded circuit on name collision, with warn-once on shadowing (the `controller_geometry.json` D6 pattern). A malformed plugin file is skipped with a warning and never kills startup.
- **Cached merged schema**: `load_schema()` becomes a cached `&'static Schema` behind a `Mutex<Option<&'static Schema>>` global (theme's pattern — `Mutex`, not `OnceLock`, so test ordering cannot poison it), initialized from `main()` before `ratatui::init()` so warnings land on a clean terminal (ADR 14). The 6 production call sites take `&Schema` today, so this is a drop-in — no call-site changes.
- **Declared metadata replaces name inference** (scope B): `CableKind::from_circuit` and `circuit_color` in `src/ui.rs` become declared fields on the circuit definition (`cable_kind`, `color`), with the substring tables kept as fallback so the 76 embedded circuits keep byte-for-byte classification.
- **Validation/latency pick plugins up free**: the 9 validation checks and the latency cost model are already schema-driven, so plugin circuits participate in `ram_overflow`, `unknown_param`, jack checks, etc. `ramsize` being required means the RAM budget and `circuit_avg` are never silently mis-summed.
- **Optional `[plugins]` config section**: plugin directory override and enable flag in `config.toml` (defaults: enabled, XDG plugins dir).

## Capabilities

### New Capabilities
- `circuit-plugins`: loading, merging, and validating user-supplied DROID circuit definitions from TOML plugin files, and the declared circuit metadata (cable kind, color) consumed by the graph/panel renderers.

### Modified Capabilities
- `patch-validation`: validation now runs against the merged schema, so plugin circuits are validated and the `ram_overflow` check no longer silently skips when a plugin circuit is present.
- `signal-flow-graph`: cable kind/color for a plugin circuit comes from its declared metadata instead of substring inference, and a plugin circuit renders with correct edge kind and node color.
- `configuration`: optional `[plugins]` section (directory override, enable flag) in `config.toml`.
- `visual-validation`: the coverage matrix gains a plugin-circuit fixture row proving declared kind/color rendering.

## Non-goals

- **Controllers**: controller plugins are a separate change — they need token synthesis (`KNOWN_CONTROLLER_SECTIONS`/`synthesize_controller_tokens` in `patch.rs`), mm geometry (`controller_geometry.json` aliases), and rack slots (`rack_geometry.json`), none of which live in the schema. Filed as a follow-up bead.
- **Hot reload**: plugin changes require an app restart (same as `config.toml`).
- **Backfilling explicit `cable_kind`/`color` onto all 76 embedded circuits**: substring fallback preserves byte-for-byte classification; backfilling is a possible follow-up but a behavior-change risk not bundled here.
- **Removing the substring inference code**: kept as fallback.

## Impact

- **Code**: new `src/plugin.rs` (+ `lib.rs` wiring); `src/schema.rs` (cached merged `Schema`, merge function, `include_str!` stays as the embedded base); `src/ui.rs` (`CableKind::from_circuit`, `circuit_color` read declared fields first); `src/config.rs` (`[plugins]` section); `src/main.rs` (`schema::init` before `ratatui::init`).
- **Consumers unchanged**: `validation.rs`, `latency.rs`, `optimize.rs`, `graph.rs` already consume `&Schema` and need no changes.
- **Dependencies**: none new (TOML already a dependency).
- **Docs**: ARCHITECTURE.md/DESIGN.md updated; specs for the four modified capabilities + new `circuit-plugins` capability.