# Circuit Plugin System — Design

## Context

The embedded schema (`ext/droid-lsp/droid-lsp/src/circuits.json`, compiled via `include_str!` in `src/schema.rs`) is the single source of truth for validation (9 checks), latency (`circuit_avg`), the optimizer, and rendering metadata. Today an unknown circuit silently disables the `ram_overflow` check (`validation.rs::ram_not_checked_when_unknown_circuit`) and falls back to latency cost `1.0` (`latency.rs`). Two `src/ui.rs` functions infer rendering metadata from circuit-name substrings: `CableKind::from_circuit` (clock/gate/trigger/pulsar/div → Control; midi/note/seq/pitch → Midi; else Audio) and `circuit_color` (button/notebuttons, pot/faderbank, …). See proposal.md — Why.

The 6 production `load_schema()` call sites (`schema.rs:355`, `schema.rs:485`, `optimize.rs:528`, `graph.rs:244`, `app.rs:1068`, `app.rs:1174`) all take `&Schema`; the remaining 25 sites are `#[cfg(test)]`. Consumers (`validate_patch(&Patch, &Schema)`, `circuit_avg(&NodeId, &Schema)`, `suggest_circuit_with_schema`) already receive the schema as a parameter, so changing the loader's return shape to a cached reference is a drop-in.

## Goals / Non-Goals

**Goals:**
- Plugin circuits load from TOML at startup and flow into validation, latency, and rendering with no per-call-site changes.
- Merge semantics: plugin overrides embedded on collision, warn-once on shadow (D6 pattern from `controller_geometry.json`).
- Embedded circuits keep byte-for-byte behavior (substring inference stays as fallback).

**Non-Goals:**
- Controller plugins (separate change: token synthesis, mm geometry, rack slots — none schema-shaped).
- Hot reload; backfilling declared metadata onto the 76 embedded circuits; removing substring inference.

## Decisions

### D1: Plugin format is TOML
The user relaxed TOML as a hard requirement ("if you have better solution I'm also fine"). TOML remains the choice: it is already a project dependency (config.rs, labels.toml), serde deserialization is free, and it matches the `config.toml`/`labels.toml` precedent. Alternatives (JSON — would match circuits.json's native shape but adds no benefit since serde handles both; INI — would collide with the `.ini` patch semantics) rejected.

### D2: Cached merged schema behind `Mutex<Option<&'static Schema>>`
`load_schema()` becomes a cached `&'static Schema` behind the same shape the theme uses (`Mutex<Option<&'static Theme>>`) — `Mutex`, not `OnceLock`, so test ordering cannot poison the palette across tests (ADR 14 rationale applies identically). Initialization is explicit from `main()` (`schema::init()` before `ratatui::init()`, ADR 14 — warnings land on a clean terminal). Fallback: if uninitialized (e.g. unit tests that never call init), `load_schema()` parses the embedded JSON on demand as today, so the 25 test sites keep working unchanged. This avoids an API change across all 33 call sites and avoids re-parsing on every call (the current per-call parse, including inside `graph.rs::compute_latency`).

### D3: Merge is an ordered overlay, not a rewrite
The embedded schema is the base layer; plugin files load in sorted filename order and insert-or-override into the circuit map. Collision → plugin wins + warn-once shadow notice per file. Malformed file → skip + warn-once, never abort startup (D6 pattern: load-time validation + fallback + warn, "never dies"). Case-insensitive circuit-name matching, same as the embedded lookup.

### D4: `ramsize` required in plugin definitions
An unknown circuit today disables the entire `ram_overflow` check and defaults latency to 1.0 — a missing or wrong `ramsize` would silently mis-sum the RAM budget. Required `ramsize` keeps RAM validation and the AVG cost model correct for plugin circuits by construction. Rejected: defaulting ramsize (silent mis-sum), or requiring users to opt into RAM checks.

### D5: Declared metadata with substring fallback
`CableKind::from_circuit` and `circuit_color` gain a declared-fields-first path: the schema's circuit definition carries optional `cable_kind` and `color` (theme token name), consulted before the substring tables. Embedded circuits declare nothing, so they keep byte-for-byte classification. Backfilling all 76 embedded circuits with explicit kinds is a possible follow-up but a behavior-change risk, deliberately out of scope.

## Risks / Trade-offs

- **[Shadowing hides a real circuit]** A plugin overriding an embedded circuit stays silently winning once the submodule catches up to a firmware that ships that circuit → warn-once on shadow at startup (D3), documented in the plugin format.
- **[Plugin with wrong ramsize mis-sums RAM]** Required `ramsize` (D4) removes the silent-default path; a wrong value is user error, surfaced by the same validation that gates loads.
- **[Mutex global test poisoning]** Same hazard the theme solved → same solution: `Mutex<Option<&'static Schema>>`, explicit init, on-demand fallback parse for uninitialized contexts.
- **[Rendering a plugin circuit with no geometry]** Plugin circuits in the physical rack view hit the existing bare-controller fallback (5 HP empty module, memory #231) — same as today for unknown controllers; acceptable and unchanged by this change.

## Migration Plan

Backward compatible: no plugin files → embedded-only behavior identical to today. A user adds files under `$XDG_CONFIG_HOME/droid-tui/plugins/` to opt in. Rollback = delete the files or set `[plugins] enabled = false`. No schema/API breaking changes; the embedded `include_str!` stays as the base layer.

## Open Questions

None — deferred unknowns (hot reload, controller plugins, backfilling embedded metadata) are explicit non-goals with follow-up beads.