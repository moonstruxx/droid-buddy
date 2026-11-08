# Project History — droid_tui

## Why

`droid_tui` accumulated its design in OpenSpec change proposals, ADR entries in `ARCHITECTURE.md`, and a
long tail of maintenance commits that are not all captured in one place. New sessions (human or agent)
otherwise rediscover the same decisions from code. This change archives the project's existing history
into OpenSpec so the rationale survives independently of any single session.

## What Changes

- Archive a concise project history: what the project is, the decisions already made, the known
  constraints and technical debt, and the current state.
- No source code, behaviour, configuration, or documentation outside `openspec/` is modified.

## Capabilities

**New Capabilities:** none.

**Modified Capabilities:** none. This change records history only; no requirement in
`openspec/specs/` changes.

## Impact

- `openspec/changes/project-history/` (this proposal), archived immediately after writing.
- No code, API, dependency, or configuration impact.

## Non-goals

- Regenerating `ARCHITECTURE.md` or `DESIGN.md` (separate `/make-architecture` and `/make-design` runs).
- Implementing any of the listed tech-debt items; they stay recorded, not scheduled.
- Backfilling the 50+ already-archived change proposals into a rewritten narrative.

## History captured

### What this project is

A single-crate Rust (2021) native desktop application for DROID (Der Mann mit der Maschine) Eurorack
hardware patch files (`.ini`). It parses patches, renders the physical rack at 1:1 millimetres, shows a
signal-flow graph, validates patches against the authoritative schema, diffs two patches, optimises
section order for loop latency, and can upload the in-memory patch to hardware over MIDI SysEx (one-way).
Stack: winit 0.30 + egui 0.36 + wgpu; no async runtime, no network, no terminal stack. Layered monolith:
`main` (winit event loop) → `app` (state) → `handler` (input) → `patch` (domain + parser) → `gui` (egui
paint); pure model modules (`schema`, `validation`, `graph`, `layout`, `geometry`, `physical`, `diff`,
`optimize`, `sysex`, `panes`, `report`) carry no UI dependency.

### Key decisions already made

- **Hand-rolled `.ini` parser** over the `ini` crate, to preserve repeated section names and precise token
  spans (the crate was removed).
- **Class-based pane layout** (`src/panes.rs`): every view belongs to `PaneClass::Big` or `Small`;
  `App.layout` is the only representation of what is open (the tiled-window-manager and its compat mirror
  were deleted).
- **The module UI is the only hardware surface**: a 1:1 millimetre rack model, rack-first from a
  configured per-row module inventory, so faceplates paint before any patch loads; the Panels view is
  retired.
- **Renderer owns geometry, handler consumes it**: paint publishes `component_rects` / `pane_hit_rects`
  per frame; hit-testing never derives positions itself. The graph is the exception — nodes are
  world-sized and hit-tested through the camera.
- **Column arrangement is the default graph layout**; the force solver and the sections ("black hole")
  solver are opt-in, deterministic per `SolveKnobs` tuple, and end in an explicit non-overlap
  post-condition.
- **Schema-authoritative validation**: the embedded `circuits.json` (from the `ext/droid-lsp` submodule)
  is the source of truth; `Error` findings gate a load, `Warning`/`Hint` do not.
- **Master-module resolution from the patch** (`rack::resolve`): the Forge's `# LABELS: master=16|18`
  declaration wins, then the single hosting model, else MASTER; conflicts are warnings, an unsatisfiable
  patch is an error.
- **One platform data directory** (`src/platform.rs`) for config, labels, favourites and plugins, with
  the environment injected so tests never touch process env; geometry is compiled in via `include_str!`.
- **One-way hardware bridge**: SyEx upload after explicit confirmation; nothing is read back.
- **Headless CLI** (`analyze`, `optimize`) beside the GUI with a versioned JSON report contract
  (`report_version` 1) and exit codes 0/1/2.
- **Semantic colour tokens only** (`theme.rs`); no hardcoded RGB outside tests.

### Known tech debt and constraints

- `ext/droid-lsp` is a vendored submodule; a fresh checkout must initialise it or the build fails.
- Cross-platform verification is CI-deep, not runtime-deep: no CI job opens a window or talks to MIDI.
- Release artifacts are unsigned/notarised-free and uninstallable beyond a zip; no Windows ARM64 build,
  no prebuilt Linux binary.
- The performance gate is Linux-only; its budgets are Linux release timings.
- `config.toml` has no migration framework (one marker, `zoom_baseline`).
- The event bus has no production subscribers; topology findings surface only through edge colouring.
- Modifier selection is single-var; additive union remains aspirational. Element and circuit selection
  are independent fields.
- Known visual gaps: banner-group titles are near-invisible at fit zoom in column/force modes; a body
  caged between immovable bodies can retain an intersection.
- A local clippy older than CI's cannot see a newer lint, so `rustup update stable` precedes trusting a
  local pass.

### Current state

Builds and tests green on Linux/Windows/macOS CI; 43 capability specs under `openspec/specs/`; ~50
archived change proposals; all recent changes archived with no active change at init time. Working tree
on `main`.
