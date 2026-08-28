# Design: patch-validation

## Context
Patch validation must be line-accurate and schema-authoritative. `droid-lsp` is the existing authority (`src/circuits.json`, `src/jacks.json`, `src/diagnostics.ts`). Re-implementing checks without the submodule would drift.

## Decisions
- D1: Vendor `droid-lsp` as `ext/droid-lsp` git submodule (commit-pinned). Embed `ext/droid-lsp/src/circuits.json` via `include_str!` in `src/schema.rs`. No file copy, no build-time fetch. Alternative rejected: npm package (JS-only, not Rust-embeddable).
- D2: `src/patch.rs` per-entry spans — `EntrySpan` with `key_span`/`value_span`/`line`. Needed because 3 checks need value-level spans (duplicate param second occurrence, invalid jack value, duplicate cable second def) and remaining need header span for missing-required. Lowercasing after span capture preserves column accuracy.
- D3: Pure `src/validation.rs` — no I/O, no App dependency. Takes `&Patch` + `&Schema`, returns sorted `Vec<ValidationIssue>`. Enables unit tests without TUI.
- D4: `load_patch` gating — `Error` => `patch=None` + `showing_validation=true` (modal blocks panels). `Warning`/`Hint` => load succeeds but modal still opens. Keeps wiring-only gating aspirational per explore.
- D5: Modal over inline status — terminal height limited, list may be 20+ issues. Modal reuses `ui.rs` overlay pattern (picker). `Enter` jumps to `source_scroll` for viewer parity.

## Risks
- Submodule pin staleness — mitigate via `cargo test` asserting `circuits.json` circuit count 76.
- RAM check `ramsize` divergence — validate against `Schema.param` `ramsize` + `JACK_TABLE` expansion.
