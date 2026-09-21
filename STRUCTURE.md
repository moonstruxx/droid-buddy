# Codebase Structure

## Directory Layout

```
droid_tui/
├── src/                        # All Rust source (single crate: lib + bin)
│   ├── main.rs                 # binary entry — winit event loop + egui paint
│   ├── lib.rs                  # library root — module wiring (pub mod …)
│   ├── app.rs                  # App state struct + tile stack + helpers
│   ├── handler.rs              # keyboard + mouse event handling
│   ├── patch.rs                # domain model + hand-rolled .ini parser
│   ├── schema.rs               # embedded DROID circuit schema + jack table
│   ├── validation.rs           # 9 check patch validation (ported from droid-lsp)
│   ├── diff.rs                 # pure patch-diff model (DiffReport)
│   ├── graph.rs                # signal-flow graph model + topology validation
│   ├── layout.rs               # column arrangement (default) + force solver
│   ├── graph_render.rs         # backend-neutral SceneSpec types
│   ├── geometry.rs             # rack-geometry model + wiring-outlier scorer
│   ├── physical.rs             # physical 1:1 layout model (mm grid)
│   ├── expression.rs           # Droid expression evaluator
│   ├── latency.rs              # forward-loop latency metric + CostModel
│   ├── optimize.rs             # latency-optimized patch generation
│   ├── config.rs               # XDG config.toml load/save
│   ├── favorites.rs            # FavoritesStore (XDG favourites.toml)
│   ├── plugin.rs               # plugin-circuit TOML discovery + parse
│   ├── theme.rs                # semantic color-token layer + palettes
│   ├── help.rs                 # help-content module (`?` modal)
│   ├── events.rs               # synchronous observer event bus
│   ├── rendermetrics.rs        # render-feature extractor + scorer
│   ├── regression.rs           # cross-layer model/story regression tests (cfg(test))
│   └── gui/                    # egui surfaces (window shell dispatch)
│       ├── mod.rs              # surface dispatch + SurfaceCtx + WindowFrame
│       ├── panels.rs           # controller panels (hardware components)
│       ├── physical.rs         # 1:1 rack view
│       ├── viewer.rs           # source pane + minimap
│       ├── picker.rs           # file picker + favourites overlay
│       ├── overlays.rs         # validation modal / select menu / labels / diff / optimizer
│       └── graph.rs            # signal-flow canvas + minimap + tooltip
├── tests/
│   └── perf_gate.rs            # profiling gate: budgeted watchdog over the scale anchor
├── fixtures/                   # test fixtures: *.ini patches, plugins/, picker_test/,
│                               #   validation/*.ini (9-check matrix), ui_review/, pdfs
├── corpus/                     # patch corpus + analysis CSVs (features.csv, rendermetrics.csv)
├── tools/                      # developer analysis tools (Python fit/build scripts + artifacts)
├── scripts/                    # developer scripts (profile-gate.sh, regenerate.sh, verify-native-change.sh)
├── assets/                     # JetBrainsMono-Regular.ttf (UI font)
├── ext/droid-lsp/              # vendored git submodule — schema/validation source of truth
├── patches_remote/             # remote DROID patch files (reference .ini patches + Generators/)
├── openspec/                   # OpenSpec change proposals (changes/) + capability specs (specs/)
├── .opencode/                  # agent orchestration config (engineers, source roots, platform)
├── .agents/skills/             # project skills (guardrails, tui, rust, openspec, …)
├── .claude/skills/             # Claude Code skills (droid-patch-format, verify, …)
├── .beads/                     # beads issue-tracker data (Dolt-backed, tooling only)
├── Cargo.toml                  # crate manifest (deps: winit, egui, egui-winit, wgpu, serde, toml)
├── Cargo.lock
├── controller_geometry.json    # per-controller cell geometry (b32 → 4×8, …)
├── rack_geometry.json          # rack layout read by geometry::load()
├── ARCHITECTURE.md             # architecture, data flows, abstractions, ADRs
├── DESIGN.md                   # UI and design-system decisions
└── target/                     # build artifacts (partially tracked — see ARCHITECTURE.md §15)
```

## Directory Purposes

**`src/`:**
- Purpose: the entire application — a single Rust crate with a `lib` target (`src/lib.rs`) and a `bin` target (`src/main.rs`).
- Contains: one module per file (snake_case), plus the `gui/` submodule for egui paint surfaces.
- Key files: `src/lib.rs` (module wiring), `src/main.rs` (event loop), `src/app.rs` (state), `src/handler.rs` (input), `src/patch.rs` (parser).

**`src/gui/`:**
- Purpose: every egui surface — the tiled main band (panels + right-column slots) and centered overlays.
- Contains: paint routines that emit `egui::Shape` and publish per-frame geometry (`component_rects`, `graph_node_rects`, `pane_rects`, `graph_canvas_px`) back to `App`.
- Key files: `mod.rs` (dispatch + `SurfaceCtx` + `WindowFrame`), `panels.rs`, `graph.rs`, `physical.rs`, `viewer.rs`, `picker.rs`, `overlays.rs`.

**`tests/`:**
- Purpose: integration tests that need the full crate.
- Contains: `perf_gate.rs` — three budgeted pure phases over the scale anchor `fixtures/droid_mpfs5melody2.ini`.
- Key files: `perf_gate.rs`.

**`fixtures/`:**
- Purpose: `.ini` patch files and supporting data exercising every parser/graph/validation/physical path.
- Contains: concrete patches (single-purpose and multi-feature), `plugins/*.toml` for the plugin loader, `validation/*.ini` for the 9-check matrix, `picker_test/`, `ui_review/`, and reference PDFs.
- Key files: `droid_mpfs5melody2.ini` (the scale anchor), `validation/*.ini`.

**`corpus/`:**
- Purpose: patch corpus plus the feature CSVs that drive the offline outlier-model fits.
- Contains: `features.csv`, `rendermetrics.csv`, and `good/*.ini` gold patches.
- Key files: `features.csv` (input to `tools/fit_outlier_model.py`).

**`tools/`:**
- Purpose: developer-only analysis tooling, not shipped in the binary.
- Contains: Python fit/build scripts and the embedded model artifacts they produce (`outlier_artifact.txt`, `influence_stats.txt`, `render_artifact.txt`).
- Key files: `fit_outlier_model.py`, `fit_render_model.py`, `build_features.py`.

**`ext/droid-lsp/`:**
- Purpose: vendored git submodule (`moonstruxx/droid-lsp`) — the authoritative circuit schema and validation reference.
- Contains: `src/circuits.json` (schema, compiled in via `include_str!`) and `src/diagnostics.ts` (validation port reference).
- Key files: `ext/droid-lsp/droid-lsp/src/circuits.json`.

## Key File Locations

**Entry points:** `src/main.rs` (winit event loop + egui paint); `src/lib.rs` (module wiring — every `src/<name>.rs` is `pub mod <name>` here).
**Configuration:** `Cargo.toml` (manifest); `src/config.rs` (XDG `config.toml` load/save); `controller_geometry.json` and `rack_geometry.json` (geometry data loaded relative to `CARGO_MANIFEST_DIR`).
**Core logic:** `src/patch.rs` (domain + parser), `src/app.rs` (state), `src/handler.rs` (input), `src/graph.rs` + `src/layout.rs` (signal-flow graph + solver), `src/schema.rs` + `src/validation.rs` (schema + validation).
**Rendering:** `src/gui/` (egui surfaces), `src/theme.rs` (color tokens), `src/graph_render.rs` (scene spec).
**Tests:** co-located `#[cfg(test)] mod tests` in every source file, plus `src/regression.rs` (cross-layer) and `tests/perf_gate.rs` (integration).

## Naming Conventions

**Files:** snake_case, one module per file; the module name matches the filename (`src/patch.rs` → `pub mod patch`).
**Directories:** snake_case for source (`src/gui/`); `.`-prefixed for tooling `.opencode/`, `.agents/`, `.beads/`, `.claude/`.
**Tests:** co-located `#[cfg(test)] mod tests` blocks in the source file under test; integration tests live in `tests/` with `_gate`/`_test`-style names where relevant.
**Symbols:** `snake_case` functions/types-local, `CamelCase` types/structs/enums, `SCREAMING_SNAKE` for `const`/`static` (e.g. `GRAPH_ZOOM_PRESETS`, `SOLVE_ITERATIONS`).

## Where to Add New Code

**New egui surface:** `src/gui/<surface>.rs` — implement a `paint_*` routine taking `&mut App` via `SurfaceCtx`, then dispatch it from `src/gui/mod.rs`.
**New domain/parser capability:** `src/patch.rs` (model type + parse function); wire additional span capture through `EntrySpan`.
**New validation check:** `src/validation.rs` — add a check to `validate_patch` and a fixture under `fixtures/validation/`.
**New graph feature:** `src/graph.rs` (model/topology) and/or `src/layout.rs` (placement); re-resolve via `App::solve_graph_positions`.
**New pure module:** `src/<name>.rs` — add `pub mod <name>;` to `src/lib.rs`; keep it UI-free (no egui/winit dependency) unless it is a `gui/` surface.
**New config key:** `src/config.rs` schema + fallback; document the key in `ARCHITECTURE.md` (§3.x, §8 environment config) and the glossary.
**Tests:** co-located `#[cfg(test)] mod tests` in the owning source file; add a regression story to `src/regression.rs` for cross-layer behavior; use `tests/perf_gate.rs` only for budgeted performance gates.
