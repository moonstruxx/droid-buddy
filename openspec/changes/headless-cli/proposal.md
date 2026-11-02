# Proposal

## Why

The app's analysis is machine-blind. Validation, master-model resolution, topology and
wiring findings, per-edge latency and the section optimizer are already pure functions —
but every one of them is reachable only through `App` + egui. The binary's sole headless
path is `--self-test`, which reports on the paint dispatch, not on a patch. An agent or a
CI job that needs to know whether a patch loads, what master it targets, how much RAM it
needs or where its latency goes has to open a window and read pixels, which is exactly
what an agent cannot do.

Why now: the pure core is already complete and cheap to expose (parse → validate →
resolve → build graph → summarize), so what is missing is an argv surface and a
serializable report, not model work. Every further AI-facing step — an MCP wrapper, an
agent that edits patches, a pre-upload gate — needs this surface first.

## What Changes

- **`analyze <patch|->`** — one headless report: patch summary, schema provenance, the
  master plan (model, source, satisfiable), RAM used against the resolved model's budget,
  section/circuit/cable/token counts, validation findings with source spans, topology
  findings, the latency summary with its worst edges, and the banner groups. A compact
  human summary by default; the machine form behind `--json`.
- **`optimize <patch|->`** — the existing optimizer exposed headlessly: the baseline and
  every candidate with before/after/delta, `--strategy banner|global|minmax`,
  `--weight 0..1`, and `--candidate N` to select one. `--write <path>` saves it (save-as
  only: never the source, auto-suffixed, reporting the path actually written);
  `--stdout` emits the reordered patch.
- **An exit-code contract** — `0` clean, `1` findings at the `--deny` threshold (default:
  any `Error`), `2` usage/IO/parse failure. stdout carries only the report; warnings and
  diagnostics stay on stderr, so `analyze` is usable as an agent or CI gate without
  parsing prose.
- **Schema parity with the app** — the CLI resolves the schema the way `main` does
  (config plus plugin overlay) and records the source and the applied plugin files in the
  report; `--no-plugins` restricts it to the embedded schema for a reproducible run.
- **Two helpers stop being private** — `clusters_from_patch` moves from `app.rs` into
  `graph.rs` as `pub` (four mirrored copies exist today, three of them in
  `tests/perf_gate.rs`), and `Patch::render_ini` becomes `pub` so `--stdout` can emit the
  reordered patch.
- **An agent skill at `.agents/skills/droid-patch-cli/`** — a short `SKILL.md` (triggers,
  command table, exit codes) plus `references/report.md` (the JSON contract) and
  `references/recipes.md` (gate a patch, diagnose a master misfit, find the worst latency
  cables, optimize then save-as, pipe a patch through stdin). A test fails when a
  top-level report key is undocumented, so the skill cannot drift from the contract.
- **`docs/cli.md`** in the user manual, linked from the manual index.
- **`openspec/config.yaml`** — the project context still claims the app is "NOT a
  hardware bridge (no MIDI, no network)", false since `midi-upload`, and its YAGNI anchor
  still calls hardware integration a deferred idea. Every future proposal reads that
  block, so both are corrected here.
- **A latency index defect is fixed, because this change depends on it** —
  `LatencyData.edge_index` indexes the cable-edge list while `Graph.edges` also carries
  `_REG:` register edges that sort among the virtual cables, so every consumer pairing an
  entry with `graph.edges[index]` read a neighbouring edge: the renderer's latency ramp
  and the optimizer's per-edge baseline were silently misattributed, and the report's
  worst-edge list would have named the wrong cables. The mapping is remapped after the
  merge and pinned by a regression test.

New types and their consumers: `report::AnalyzeReport` / `report::OptimizeReport`
(serialized by `cli`, asserted by `tests/cli.rs`), `cli::Command` / `cli::Invocation`
(consumed by `main`'s dispatch), `cli::ExitCode` (the process exit). No new derive lands on
an existing model type: the report is its own contract, so the internal `Patch`/`Graph`
shape stays free to change.

## Capabilities

### New Capabilities

- `headless-cli`: the no-window command surface, the report contract, the exit-code and
  stream discipline, schema provenance, and the agent skill that documents it.

### Modified Capabilities

- None.

## Impact

- New: `src/report.rs`, `src/cli.rs`, `tests/cli.rs`,
  `.agents/skills/droid-patch-cli/` (`SKILL.md`, `references/report.md`,
  `references/recipes.md`), `docs/cli.md`.
- Touched: `src/main.rs` (dispatch between `schema::init` and the window), `src/lib.rs`
  (module wiring), `src/graph.rs` (public cluster helper plus the latency edge-index
  remap), `src/patch.rs` (`render_ini` visibility), `src/app.rs` (calls the moved helper),
  `tests/perf_gate.rs` and `tests/force_arrangement_invariants.rs` (drop their mirrors),
  `docs/README.md`, `openspec/config.yaml`.
- **BREAKING** (crate-internal only): `app::clusters_from_patch` disappears in favour of
  `graph::clusters_from_patch`; `Patch::render_ini` widens from `pub(crate)` to `pub`.
  Nothing user-facing breaks.
- Unchanged: GUI keybindings and pane behaviour, patch parsing and output, the validation
  and topology finding sets, the optimizer's strategies, the writer's save-as semantics,
  and `--self-test`. The one rendering-visible change is the corrected latency ramp, which
  now colours each edge by its own latency instead of a neighbour's.
- `ARCHITECTURE.md` is updated at archive, not hand-edited.

## Non-goals

- No MCP server, HTTP endpoint, IPC or daemon. The contract is argv, stdout and exit
  codes; an MCP wrapper is a later change that would sit on top of it.
- No additional subcommands: `diff`, `validate`, `sysex` and `render` are follow-ups the
  report contract makes cheap, not part of this one.
- No JSON Schema file and no machine-readable configuration; `report_version` plus
  `docs/cli.md` are the contract.
- No change to validation, topology, master resolution, latency or optimizer semantics —
  the CLI reports what the GUI already computes.
- No change to the GUI, keybindings, panes, theme or rendering; no new config keys.
- No stdin write-back: `--write` needs a real source path, so a `-` input can only
  `--stdout`.
- No machine-wide skill install; the skill ships project-local.
