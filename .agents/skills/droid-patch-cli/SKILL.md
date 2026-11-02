---
name: droid-patch-cli
description: Use when inspecting, validating or optimizing DROID .ini patch files without the GUI, via the `droid_tui analyze` and `droid_tui optimize` commands. Trigger on "validate this patch", "why won't this patch load", "what master does this target", "how much RAM does this use", "where does the latency go", "reduce patch latency", "reorder sections", "check a patch in CI", or any request to analyze a DROID patch from a script or an agent.
---

# DROID patch CLI

`droid_tui` has two headless subcommands. They read a patch, run the same code
the desktop app runs, and print a report — no window, no GPU, no display. Use
them instead of asking the user to look at the app.

## First step

Run the analysis before touching a patch:

```bash
droid_tui analyze <patch.ini> --json
```

`--json` is the form you want: it is one document with a stable contract. The
default output is a human summary.

## What the two commands answer

| question | command |
|---|---|
| Will this patch load? What is wrong with it? | `analyze` → `validation`, `topology` |
| Does it target MASTER or MASTER18, and does that fit? | `analyze` → `master` |
| How much RAM does it need? | `analyze` → `ram` |
| Where does the latency go? | `analyze` → `latency` |
| Can it be reordered to run faster? | `optimize` |
| What does the reordered patch look like? | `optimize --stdout` / `--write` |

## Command surface

```
droid_tui analyze  <patch|-> [--json] [--no-plugins] [--deny <error|warning>]
droid_tui optimize <patch|-> [--json] [--no-plugins]
                              [--strategy <banner|global|minmax>]
                              [--weight <0.0..=1.0>] [--candidate <n>]
                              [--write <path> | --stdout]
```

- `<patch>` is a path; `-` reads standard input (`generate | droid_tui analyze - --json`).
- `--json` prints the machine contract; without it you get a short text summary.
- `--no-plugins` ignores the user's plugin circuits for a reproducible run.
  The report records which schema was used (`schema.source`, `schema.plugins`).
- `--deny warning` raises the analysis gate from errors to warnings.
- `--strategy`, `--weight` and `--candidate` tune the optimizer; `--candidate 0`
  is the best one.
- `--write <path>` saves the selected candidate (save-as only: never the source,
  auto-suffixed on collision, and the report names the path actually written).
- `--stdout` prints the selected candidate's patch text instead — capture it
  with `> out.ini`. It cannot be combined with `--json`.

## Exit codes

| code | meaning | what to do |
|---|---|---|
| `0` | completed, nothing at or above the threshold | carry on |
| `1` | `analyze` found something at or above the threshold | read `validation` / `topology` in the report |
| `2` | could not run: bad arguments, unreadable file, unparsable patch | read stderr; do not retry the same command |

Standard output holds only the report (or the patch for `--stdout`). Everything
usable as a diagnosis — parse errors, usage text, warnings — is on stderr.

`optimize` never exits `1`: it reorders what it is given, and refuses `--deny`.

## Reading the report

Full field-by-field contract: [`references/report.md`](references/report.md).
Worked flows for the common questions:
[`references/recipes.md`](references/recipes.md).

The short version: `patch` is what was read, `master` is which master model the
patch resolves to and on what evidence, `ram` compares usage against that
model's budget, `validation` and `topology` are the findings (each validation
finding carries `line`/`col_start`/`col_end`), `latency` is the forward-loop
cost with its slowest edges, and `groups` are the banner groups.

## Rules

- Trust the report's own numbers over any estimate; it is computed from the
  patch and the embedded schema, not guessed.
- `optimize` does not validate. Run `analyze` first if the patch is untrusted,
  and confirm with `analyze` again after applying a reordering.
- The CLI never overwrites a patch. To keep a reordering, write it to a new file
  (`--write`) or redirect `--stdout`; editing the source is a separate decision.
- Reports are deterministic: same patch + same schema ⇒ byte-identical output.
  When two runs differ, compare `schema.source` and `schema.plugins` first.

## Related

- User manual: [`docs/cli.md`](../../../docs/cli.md).
- The app's own surfaces for the same data: the signal-flow graph (latency
  colours, latency optimizer pane) and the validation modal, see
  [`docs/views.md`](../../../docs/views.md).
