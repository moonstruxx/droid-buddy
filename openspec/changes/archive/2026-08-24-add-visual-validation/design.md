## Context

See `proposal.md` — Why. Current state: `src/regression.rs` drives `handler::handle_event` + `ui::render` through `ratatui::backend::TestBackend` into a `Buffer` and asserts per-cell text/style. `DESIGN.md` tokens already define the face (spacing 16×3, ANSI-16 palette, flat/bold/dim/reversed) and `openspec/specs/controller-panels`, `viewer-layout`, `theming`, `shift-visualization` define spec-level scenarios. `verify` skill shows `screen` cannot reliably resize (`SIGWINCH` not delivered); only the `pty` probe works. Change `2026-08-24-boxed-components-and-viewer-split` and `2026-08-24-embedded-source-navigation` left 6 open visual bugs (droid_tui-p2x..6vu) that a face-level gate would have caught. Tech stack: single-crate Rust, ratatui 0.29, crossterm 0.28, no async, no network.

## Goals / Non-Goals

**Goals:**
- Make the UI face inspectable side-by-side per theme/width without a live terminal.
- Fail `cargo test` on face regression (strict gate).
- Keep worktree clean (ephemeral) while making the archive the durable visual history.
- Reuse `TestBackend` determinism and existing `buffer_for` collaboration.

**Non-Goals:**
- Pixel images, live capture, committed golden snapshots, or new runtime deps — see proposal Non-goals.

## Decisions

**D1 — `insta` for golden management, not DIY file compare.**
Why: `cargo insta review` / `cargo insta test` is the standard Rust snapshot UX, handles multi-line ANSI without custom diffing, and integrates with `cargo test` gating. Alternative DIY (`assert_eq!` against committed `.ansi` files) would reimplement diff display and `INSTA_UPDATE` semantics.
Rejected: DIY appended to `regression.rs` only — more code, weaker UX.

**D2 — ANSI snapshot as gate, HTML gallery as proof (hybrid).**
Why: ANSI is diff-friendly (CI can `git diff` the artifact if needed); HTML is human-inspectable (color, reversed, dim). One source `Buffer` produces both. Using only one would lose either gate or inspectability.
Alternative: PNG via `terminal-to-html` → image diff — flaky, extra deps.

**D3 — `TestBackend` determinism over `pty` capture.**
Why: `TestBackend` is synchronous, no terminal IO, same `ui::render` path, width/height are explicit parameters. `pty` capture is non-deterministic (escape-sequence timing, resize race seen in `verify` skill).
Alternative: `pty` snapshot — rejected as gate source, kept only for manual smoke.

**D4 — Ephemeral worktree + durable archive.**
Why: keeps `master` free of generated snapshots (hygiene debt: `target/` is already partially tracked) while `openspec archive` copies `evidence/gallery/` into `openspec/changes/archive/<name>/evidence/` where history is intentional. CI uploads the same gallery as artifact for PR review without a commit.
Trade: PR-time `git diff` of snapshots is not available; reviewer uses CI artifact + `cargo insta review` locally.

**D5 — Coverage `start_small`: 3 fixtures × 3 themes × 2 widths × {viewer closed/open, shift active} ≈ 18 pages.**
Why: covers the 6 visual bugs (truncation, boxed border, shift chip, monotheme, viewer split) with minimal pages. Exhaustive matrix would be pure verification churn.
Expansion is a later change.

**D6 — `auto` workflow: `cargo test` generates + asserts.**
Why: matches TDD London — face is part of the executable spec; no separate `make snapshots` step to forget. The ephemeral gallery can be materialized on demand (`cargo run --bin snapshot-gallery` or `cargo test -- --generate-gallery` flag) but is not required for the gate.

## Risks / Trade-offs

- [Snapshot churn on every spacing tweak] → Mitigation: keep matrix small; grouping by panel (not per-pixel) reduces churn; `insta` inline snapshots show diff at failure site.
- [ANSI line-ending / trailing-space sensitivity causes false failures] → Mitigation: normalize buffer rows (trim trailing empty cells) before snapshotting, same as `rendered_text` helpers.
- [HTML writer drift from ratatui style] → Mitigation: single `buffer_to_html` helper with exhaustive `Style` mapping (fg/bg/bold/dim/reversed/underline), unit-tested against known cells; no ad-hoc HTML elsewhere.
- [Archive gallery size over time] → Mitigation: only HTML+ANSI (text), ~few KB per page; no images.
- [Insta file ignored but gate expected to fail] → Mitigation: CI runs `cargo insta test --check` (fails if snapshot would be created), uploads pending snapshots as artifact; local `cargo insta accept` is only for intentional change and triggers a new CI run — no commit needed.

## Migration Plan

1. Add `insta` dev-dependency, `.gitignore` entries.
2. Add snapshot helper + coverage cases; `cargo test` now fails until initial acceptance — developer runs `cargo insta review` locally to accept baseline (ephemeral, not committed).
3. CI uploads `evidence/gallery/` + pending `.snap` artifact.
4. On `openspec archive add-visual-validation`, copy `evidence/gallery/` into `openspec/changes/archive/add-visual-validation/evidence/`.
5. Rollback: revert the 4 files (`Cargo.toml`, `src/regression.rs`, `.gitignore`, CI config) — no runtime behavior to migrate.

## Open Questions

- None — all 6 exploration picks are locked. Gallery output path (`evidence/gallery/` vs `target/gallery/`) is an implementation detail that does not change specs or tasks.
