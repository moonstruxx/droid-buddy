## Why

The repo adopted **YAGNI** as a semantic anchor: every type, variant, derive, and trait must have a current consumer or it gets deleted. Three pieces of dead code violate this: the `ComponentState::Active` variant (defined, never constructed or matched), serde `Serialize`/`Deserialize` derives on all five domain types (zero serialization usage anywhere), and `App::load_sample_patch` (test-support only, living in production code). Removing them now keeps the codebase honest and prevents speculative persistence from becoming a de-facto API.

## What Changes

- **BREAKING** (internal API): remove the `ComponentState::Active` variant from `src/patch.rs` — no code constructs or matches it.
- Remove serde `Serialize`/`Deserialize` derives from `Patch`, `HwComponent`, `ComponentKind`, `ComponentState`, `ShiftGroup` in `src/patch.rs`.
- Remove `serde` and `serde_json` from `Cargo.toml` and `Cargo.lock` (no remaining usage after derive removal).
- Remove `App::load_sample_patch` from `src/app.rs`; UI tests load the fixture directly via `Patch::from_ini_file("fixtures/arpeggio1.ini")`.
- Regenerate `ARCHITECTURE.md` (serde references in §7/§16) via `/make-architecture` — derived docs are regenerated, never hand-edited (DRY/CodeAsDoc).

## Capabilities

### New Capabilities

None — pure refactor, no new behavior.

### Modified Capabilities

None — no spec-level behavior changes. The change opts out of specs via `skip_specs: true` in `.openspec.yaml`.

## Impact

- `src/patch.rs` — enum variant removal, derive removal
- `src/app.rs` — `load_sample_patch` removal
- `src/ui.rs` — three test call sites switch to fixture loading
- `Cargo.toml` / `Cargo.lock` — drop `serde`, `serde_json`
- `ARCHITECTURE.md` — regenerated (serde references)
- Tests — 24 existing tests must stay green; no behavior change expected