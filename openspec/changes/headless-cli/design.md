# Design

## Context

See `proposal.md` — Why.

The shape of the existing code decides most of this design:

- `src/main.rs` already runs a straight prologue before any window exists: load config,
  `theme::init`, `schema::init` (embedded schema plus plugin overlay), install the panic
  hook, then either `--self-test` or the winit loop. A command surface slots into that
  prologue, so it inherits the app's schema for free.
- The analysis inputs are pure and already assembled in one place per capability:
  `validation::validate_patch(&Patch, &Schema)`, `rack::resolve(&Patch, &Schema)`,
  `Graph::build_from_patch(patch, clusters, &CostModel, &GraphOptions)` (which carries
  topology, wiring-outlier, influence and latency results), and
  `optimize::generate_candidates{,_weighted}`.
- `Patch` and its domain types already derive `serde::Serialize`; `Graph`, the validation
  findings, the topology findings, `DiffReport` and `MasterPlan` do not.
- `clusters_from_patch` is private in `app.rs` and mirrored **four times** — three in
  `tests/perf_gate.rs`, one in `tests/force_arrangement_invariants.rs` — each carrying a
  "mirrors app::clusters_from_patch" comment.
- `Patch::render_ini` is `pub(crate)`; `Patch::write_to_ini` is `pub`.

## Goals / Non-Goals

**Goals:**

- One binary, two new subcommands, zero new dependencies and zero new model types beyond
  the report.
- A report contract that is small enough to read and stable enough to depend on.
- Deterministic, diffable output for the same input.
- Exit codes usable as a gate without parsing prose.

**Non-Goals:**

- Streaming or incremental output; a patch is small and one report fits in memory.
- Multi-patch batch mode, watch mode, or a config profile for the CLI.
- Any rendering, theme or layout input — the report describes the patch, not the view.

## Decisions

### D1. Subcommands inside the existing binary

`droid_tui analyze …` and `droid_tui optimize …` rather than a second binary target.
Distribution ships one artifact per platform and one code path builds the schema; a
second binary would double both and could drift on schema/plugin handling.

*Alternative rejected:* a separate `droid-analyze` bin — cleaner argv separation, but a
second `schema::init` prologue and a second entry in the release zips for no capability.

Dispatch order in `main`: config → `schema::init` → **CLI dispatch** → `--self-test` →
window. Keeping the CLI before the panic hook and window machinery means an analysis run
cannot allocate a window even by accident, and `--self-test` keeps its current semantics
(it deliberately exercises the paint path).

*Argv precedence:* a first argument that exactly matches a subcommand name is the
subcommand; `--` escapes it. The alternative — "subcommand only when a second argument
exists" — makes `analyze` alone ambiguous and would silently open a window in CI.

### D2. A curated report type, not a serialized model dump

`src/report.rs` defines `AnalyzeReport` / `OptimizeReport` with their own serde structs,
including `report_version: u32`.

`Patch` already derives `Serialize`, so dumping it was the cheap option. It was rejected
because that serialization *is* the internal model: `raw_lines`, per-entry spans,
`cable_index` and modifier indexing would become a public contract, every internal rename
would break agents' parsers, and the payload is mostly data the report does not need. The
curated report names the six facts an agent asks for and nothing else, and it can be
rounded, sorted and versioned at the boundary.

*Consequence accepted:* an internal change that alters a reported fact still needs a
deliberate report change — that is the point.

### D3. `--json` is explicit; the default is a short human summary

The binary is a human tool first. `analyze p.ini` prints counts, the master verdict, the
latency summary and a severity tally; `--json` prints the contract. Auto-detecting a TTY
was rejected: it makes the output type depend on the environment, so a redirected run and
an interactive run disagree, and the integration test would have to fake a terminal.

*Consequence accepted:* an agent must pass `--json`. That is one flag, and the skill says
so in its first example.

### D4. Exit codes `0` / `1` / `2`, with `--deny`

`0` completed and nothing at or above the threshold, `1` completed with findings at or
above it, `2` could not run. The threshold defaults to error severity, so a clean parse and
a warning-only patch both exit `0`; `--deny warning` tightens it.

This deliberately differs from `App::load_patch`, which *rejects* a patch carrying an
Error finding (`patch = None`). The GUI gates so the user cannot interact with a broken
patch; the CLI must still report the findings that justify the gate — a gate that reports
nothing is unusable. The two behaviours are documented side by side so a later
"unification" cannot quietly delete the report.

*Alternative rejected:* a single non-zero code — loses the difference between "this patch
has problems" and "I could not look at this patch", which is the distinction an agent loop
branches on.

### D5. Schema parity plus an explicit escape hatch

The CLI loads `config.toml` and overlays plugin circuits exactly as the app does, because
a report that disagrees with the GUI is worse than no report. Provenance is recorded
(`schema.source`, `schema.plugins`) so a surprising verdict is explainable, and
`--no-plugins` gives a reproducible run when the user's plugin directory is not part of the
question.

*Alternative rejected:* embedded-only by default — reproducible, but the CLI would report
an "unknown circuit" error for a patch the GUI happily opens.

### D6. `clusters_from_patch` moves to `graph.rs`

The graph build needs clusters and `App` owns a private mapping from banner groups. A
CLI/report path would be the fifth copy. Moving it next to `Cluster` makes it the one
definition, and the two integration tests drop their mirrors — a DRY fix the change would
be paying for anyway.

### D7. `render_ini` widens to `pub`; the writer keeps its rules

`--stdout` needs the rendered text, which only `render_ini` produces. It becomes `pub`
(documented as lossless, current section order). `--write` reuses `write_to_ini`, so all
existing writer guarantees hold unchanged: save-as only, source-path refusal by
canonicalized comparison, atomic temp-then-rename, auto-suffix on collision.
`write_to_ini` already returns the path it wrote, which is what the report's `wrote` field
carries — no second "did the name change?" computation.

### D8. Standard input is read-only

`-` reads a patch from stdin and behaves identically to a file, because piping a generated
patch without touching the filesystem is the common agent flow. `--write` needs a real
source path (it refuses to overwrite the source and derives the suffix from the
destination), so a stdin input is rejected for `--write` with a message naming the reason,
and `--stdout` remains available.

*Alternative rejected:* fabricating a source path for stdin — the refusal check would then
compare against a path the user never named.

### D9. The skill is documentation with a test attached

The skill lives at `.agents/skills/droid-patch-cli/` with progressive disclosure:
`SKILL.md` (triggers, command table, exit codes, link to `docs/cli.md`),
`references/report.md` (the JSON contract) and `references/recipes.md` (worked flows). One
skill, not two: `analyze` and `optimize` share the exit-code and report contract, and a
skill's value is its trigger description, which covers both workflows.

To stop the reference rotting, one test asserts every top-level key of a serialized
`analyze` report appears in `references/report.md`. It is a string-containment check, not a
parser, so it stays honest without turning the skill into a spec.

*Alternative rejected:* generating `references/report.md` from the structs — removes the
prose that makes the reference useful to an agent.

### D10. Determinism is a property of the report boundary

Findings are already sorted by `(line, column)`, graph edges by `(cable, source, sink)`,
banner groups are ordered, and plugin files load in sorted-filename order. The report
therefore only has to (a) keep those orders, (b) round latency to four decimals at
serialization so an f32's full expansion never enters the contract, and (c) bound the
worst-edge list deterministically (sort by latency descending, tie-break by cable then
endpoints). Two runs on one patch then produce byte-identical output.

## Risks / Trade-offs

- **[The CLI diverges from the GUI's view of a patch]** → Both paths call the same pure
  functions with the same schema; the divergence risk is the schema, which D5 pins by
  construction and records in the report.
- **[A new subcommand could open a window in CI and hang]** → Dispatch happens before any
  window request, and `tests/cli.rs` runs the real binary; a window attempt would fail on a
  headless runner rather than pass silently.
- **[The report becomes an accidental API and freezes internals]** → D2 keeps it curated
  and versioned; `report_version` is the only compatibility lever, and the drift test ties
  the documentation to the emitted keys.
- **[Rounding hides a real difference]** → Four decimals is far finer than the metric's
  meaningful resolution (per-edge latency is a distance count times a per-circuit average);
  the raw f32 is still printed in the text summary's worst-edge line.
- **[Plugin overlay makes two machines disagree]** → `schema.plugins` names the applied
  files and `--no-plugins` exists; the report is explainable even when it is not identical.
- **[`--stdout` and `--write` on the same invocation]** → Exactly one output option acts;
  passing both is a usage error (exit `2`) rather than a silent choice.

## Migration Plan

Additive: no config key, no stored data, no schema change. Existing invocations
(`droid_tui`, `droid_tui <patch>`, `droid_tui --self-test`) keep their behaviour, so there
is nothing to migrate or roll back beyond reverting the change.

`ARCHITECTURE.md` and `DESIGN.md` are regenerated at archive, not hand-edited here.
