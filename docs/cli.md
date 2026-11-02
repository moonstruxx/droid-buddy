# Headless CLI

`droid_tui` also runs without a window, as two subcommands: `analyze` reports
what a patch is and what is wrong with it, `optimize` proposes section
reorderings that reduce forward-loop latency. Both read a patch, run the same
code the app runs, and print a result — no window, no GPU device, no display.

This is the surface for scripts, CI and AI agents. Everything is decided by
argv, stdout and the exit code.

```
droid_tui analyze  <patch|-> [--json] [--no-plugins] [--deny <error|warning>]
droid_tui optimize <patch|-> [--json] [--no-plugins]
                              [--strategy <banner|global|minmax>]
                              [--weight <0.0..=1.0>] [--candidate <n>]
                              [--write <path> | --stdout]
```

`<patch>` is a path; `-` reads the patch from standard input. A first argument
that spells `analyze` or `optimize` is the subcommand; `--` ends option parsing,
so `analyze -- optimize` reads a file named `optimize`. The bare
`droid_tui [patch.ini]` form still opens the window.

## analyze

```
$ droid_tui analyze fixtures/own_buttons.ini
own_buttons · 10 sections · 9 circuits · 8 cables · 19 hw tokens
master: MASTER (default)
ram: 1000/112867 bytes (MASTER)
latency: avg 0.0074 · max 0.0074 · 0 back edges
  worst: _OWN_A  button 1 → copy 1  0.0074
schema: 76 circuits (embedded)
validation: 0 error(s), 0 warning(s), 7 hint(s)
  14:13  hint  unused_cable  Virtual cable "_OWN_B" is defined in [button] but never used
  …
topology: 0 finding(s)
groups: 1
  (unnamed)  [0..10)
```

The report covers the patch summary, the resolved master model and how it was
resolved, RAM used against that model's budget, the forward-loop latency
summary and its slowest edges, where the schema came from, every validation
finding with its source position, every topology finding, and the banner
groups.

`--json` prints the same facts as one machine-readable document:

```json
{
  "report_version": 1,
  "tool": { "name": "droid_tui", "version": "0.1.0" },
  "patch": { "path": "fixtures/own_buttons.ini", "name": "own_buttons",
             "bytes": 759, "sections": 10, "circuits": 9,
             "cables": 8, "hw_tokens": 19 },
  "schema": { "circuits": 76, "source": "embedded", "plugins": [] },
  "master": { "model": "MASTER", "source": "default", "satisfiable": true },
  "ram": { "used": 1000, "budget": 112867, "model": "MASTER" },
  "validation": [ { "severity": "hint", "code": "unused_cable",
                    "message": "…", "line": 14, "col_start": 13, "col_end": 21 } ],
  "topology": [ { "cable": "_X", "severity": "error", "message": "…" } ],
  "latency": { "avg": 0.0074, "max": 0.0074, "back_edges": 0,
               "worst": [ { "cable": "_OWN_A", "source": "button 1",
                            "sink": "copy 1", "latency": 0.0074 } ] },
  "groups": [ { "title": "(unnamed)", "start": 0, "end": 10 } ]
}
```

Field notes:

- `report_version` is the contract version. It changes only when a field's
  meaning or presence changes.
- `severity` is `error`, `warning` or `hint`; `master.source` is `declared`,
  `inferred` or `default`; `master.satisfiable` is `false` when no master model
  can host the patch at all.
- `line` is 1-based, `col_start`/`col_end` are 0-based byte columns into that
  line.
- `latency.*` and `worst[].latency` are rounded to four decimals, and `worst`
  is the slowest edges only, worst first (capped at ten).
- `schema.source` is `embedded` or `embedded+plugins`, and `schema.plugins`
  names every plugin file that was applied.

## optimize

```
$ droid_tui optimize fixtures/optimizer_latency.ini
optimizer_latency · 5 sections · 0 circuits · 4 cables · 2 hw tokens
baseline: avg 4.0000 · max 4.0000 · 4 back edges
[0] banner (default)  avg 4.0000 → 1.0000 (-3.0000)  max 4.0000 → 1.0000 (-3.0000)
```

- `--strategy banner|global|minmax` — banner-preserving min-sum (default),
  whole-file min-sum, or all strategies including the min-max objective.
- `--weight <0.0..=1.0>` — trade total latency against the worst edge;
  `0.0` is the app's default objective.
- `--candidate <n>` — which candidate an output option acts on, `0` = best.
- `--write <path>` — save that candidate as a patch file. This is save-as only:
  the source path is refused, an existing destination is never overwritten but
  suffixed (`p.ini` → `p-1.ini`), and the report's `candidates[n].wrote` names
  the path actually written.
- `--stdout` — print that candidate's patch text instead of a report, so
  `droid_tui optimize p.ini --stdout > reordered.ini` works. `--stdout` and
  `--json` together are a usage error, because a JSON report on the same stream
  would corrupt the patch.

`optimize` is not gated by validation: it reorders whatever it is given, and
`--deny` is rejected for it. Run `analyze` when you want a verdict.

## Exit codes

| code | meaning |
|---|---|
| `0` | the command completed and no finding reached the deny threshold |
| `1` | `analyze` completed and a finding did reach it |
| `2` | the command could not run: bad arguments, unreadable file, unparsable patch |

Standard output carries only the report (or the emitted patch for `--stdout`).
Warnings, parse errors and usage text go to standard error, and standard output
is empty whenever the command exits `2`.

The threshold defaults to `error`, so a patch with warnings still exits `0`.
`--deny warning` raises it. This differs on purpose from the app, which refuses
to *load* a patch with an error finding: the CLI reports the findings that
justify a refusal, because a gate that prints nothing cannot be acted on.

## Schema and plugins

The CLI installs the schema the same way the app does: the embedded
`circuits.json` plus the circuits in
`<data dir>/droid-tui/plugins/*.toml`, and the `[plugins]` section of
`config.toml` applies. A report therefore never disagrees with what the window
shows. The report records which schema was used, and `--no-plugins` restricts a
run to the embedded schema for a reproducible result.

`config.toml`'s `[latency] per_circuit` overrides apply to both commands.

## Standard input

`-` reads the patch from standard input, which is the convenient form for an
agent that generated a patch and does not want to write it to disk first:

```
generate_patch | droid_tui analyze - --json
```

Standard input has no path, so `--write` is rejected for it; `--stdout` still
works.

## Determinism

The same patch with the same schema produces byte-identical output. Findings
keep source order, edges keep their deterministic order, latency is rounded
before serialization, and plugin files are listed in load order. Two runs that
differ are explained by `schema.source`/`schema.plugins`.

## Related

- [Views](views.md) — the same analysis inside the app: the signal-flow graph,
  the validation modal and the latency optimizer pane.
- [Configuration](config.md) — `[plugins]` and `[latency]` keys this CLI honours.
- `.agents/skills/droid-patch-cli/` — the agent skill (triggers, recipes) built
  on this page.
