## Why

UI is the product for `droid_tui` — a flat ANSI-16 terminal panel that mirrors physical DROID hardware. Today correctness is asserted only by per-cell `TestBackend` buffer checks (`src/regression.rs`) and ephemeral manual `screen`/`pty` runs. Style-correct does not mean face-correct: the P2B8 truncation bug ("P2B8 Button 1." for all 8 buttons, see droid_tui-p2x) passes cell asserts but is indistinguishable to a human. There is no inspectable face, no spec-to-face trace, and no durable proof that "this is what the UI looked like at archive time" — so UI evolution cannot be tracked.

## What Changes

- Add an **inspectable visual proof** pipeline: `TestBackend` → ANSI snapshot + HTML gallery rendered from the same buffer (span per cell with fg/bg/bold/dim/reversed), side-by-side per theme.
- Generate a **small, deterministic coverage matrix** (~18 pages): fixtures `arpeggio1.ini`, `led_pairs.ini`, `source_navigation.ini` × themes `classic`/`terminal`/`mono` × widths `80`/`120` × viewer open/closed and `shift1` active. No live terminal capture.
- Wire a **strict `cargo test` gate**: any snapshot mismatch fails the test run (`insta` inline snapshot, `cargo insta test` as source of truth). No advisory-only mode.
- Make the gallery **ephemeral in the worktree** (generated, `.gitignore`'d, uploaded as CI artifact) and **durable in the archive**: on `openspec archive` the gallery is copied to `openspec/changes/archive/<change>/evidence/gallery/` so history is preserved without committing snapshots to `master`.
- Adopt `insta` for golden-file management (`cargo insta review` workflow) and auto regeneration on `cargo test` (`auto` workflow).

## Capabilities

### New Capabilities

- `visual-validation`: Inspectable UI face, spec-to-face comparison, and archive-bound visual proof for every change. Covers snapshot generation, HTML gallery, strict gate, ephemeral storage + archive durability, and coverage policy. Use kebab-case path `visual-validation` per existing `openspec/specs/` organization.

### Modified Capabilities

<!-- none — existing capability requirements (controller-panels, viewer-layout, theming, shift-visualization) do not change; they gain proof, not new behavior -->

## Impact

- `Cargo.toml` / `Cargo.lock`: add `insta` (dev-dependency) for snapshot management; no runtime dep.
- `src/regression.rs`: add snapshot helper (`buffer → ANSI + HTML` rendering) and coverage cases for the matrix; existing buffer helpers (`buffer_for`) reused.
- `.gitignore`: ignore `snapshots/` (insta output) and `evidence/gallery/` ephemeral output.
- `openspec/changes/add-visual-validation/evidence/` and `openspec/changes/archive/*/evidence/`: gallery HTML + ANSI artifacts (durability target).
- CI: ephemeral artifact upload (gallery + ANSI) — no committed snapshots on `master`.
- Derived docs: `DESIGN.md` provenance note, `ARCHITECTURE.md` testing strategy note — regenerated via `/make-design`/`/make-architecture` (not hand-edited).
- Behavior: no user-visible TUI change; pure verification addition.

## Non-goals

- No PNG/pixel comparison and no screenshot image generation — ANSI+HTML spans are sufficient (YAGNI; avoids image-diff flake and extra tooling).
- No live terminal or `pty` capture — determinism of `TestBackend` is required; `verify` skill remains manual smoke only.
- No committed `snapshots/` golden on `master` — ephemeral by decision `3:ephemeral`; archive is the durable proof.
- No hardware integration, no persistence schema, no new component kinds or `ComponentKind` variants — no new type without a consumer.
- No exhaustive combinatorial coverage — `start_small` matrix only; expansion is a future change if needed.
