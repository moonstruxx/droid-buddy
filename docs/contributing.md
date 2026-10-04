# Contributing

Deep onboarding for contributors. The repo-root `README.md` carries
the short version; this page is the complete one.

## Setup

```sh
git clone <repo> && cd droid_tui
git submodule update --init   # ext/droid-lsp: the circuit-schema source of truth
cargo build
```

`ext/droid-lsp` is a vendored git submodule — without it the build
has no schema to embed. The binary runs with `./target/release/droid_tui
[patch.ini]`, falling back to an embedded demo patch.

## The four gates

All four must exit 0 before reporting completion:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test
cargo build --release --locked
```

Tests are co-located `#[cfg(test)]` unit tests plus
`src/regression.rs` (cross-layer stories) and `tests/perf_gate.rs`
(budgeted scale-anchor phases). Rendering is verified by headless
egui shape/label assertions — no GPU, no live window.

## Derived docs rule

`ARCHITECTURE.md` and `DESIGN.md` are **generated** artifacts — never
hand-edit them. Regenerate via the `/make-*` commands. During OpenSpec
archive, the docs hand-edit step is skipped; flag stale wording in
the report for regeneration instead of patching inline.

## Change workflow (OpenSpec + beads)

- Propose under `openspec/changes/`, implement, archive to
  `openspec/changes/archive/`, sync capability specs to
  `openspec/specs/`. No behavior change without a spec delta.
- All task tracking goes through `bd` (beads): `bd ready` for
  available work, `bd update <id> --claim`, `bd close <id> --reason`.
  No markdown TODO lists.
- Docs-only changes (like this manual) touch `docs/**` only and carry
  no spec deltas.

## Where code goes

Single crate, one module per file (`src/<name>.rs` + `pub mod` in
`src/lib.rs`): `patch.rs` (parser), `app.rs` (state), `handler.rs`
(input), `graph.rs` + `layout.rs` (signal flow), `schema.rs` +
`validation.rs` (schema + checks), `gui/` (egui surfaces). New pure
modules stay UI-free; new surfaces dispatch from `src/gui/mod.rs`.
See `STRUCTURE.md` for the full map.
