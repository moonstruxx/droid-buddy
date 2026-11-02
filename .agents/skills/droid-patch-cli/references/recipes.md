# Recipes

Worked flows for the questions the CLI exists to answer. Every command assumes
`droid_tui` on `PATH`; the `jq` snippets are conveniences, not requirements —
the raw document is the same data.

## Gate a patch in CI or a pre-commit hook

```bash
droid_tui analyze patch.ini --json > /dev/null
# exit 0 = loads, 1 = error findings, 2 = could not run
```

Raise the bar to warnings only where warnings are actionable:

```bash
droid_tui analyze patch.ini --json --deny warning > /dev/null
```

To keep the report for the build log, write it to a file and keep the exit
code:

```bash
droid_tui analyze patch.ini --json > report.json || echo "patch has findings"
```

## Answer "why won't this patch load"

```bash
droid_tui analyze patch.ini --json | jq '.validation[] | select(.severity=="error")'
```

Each finding names its `code`, its `message` and its position (`line`,
`col_start`). The codes worth recognising:

- `unknown_circuit` — the circuit name is not in the schema (and the message
  suggests the closest one).
- `unknown_param` — the parameter does not exist on that circuit.
- `missing_required` — a required input is unset.
- `ram_overflow` — the patch exceeds the resolved master's RAM.
- `no_master_fits` — no master model can host the patch.
- `invalid_expression` — a parameter value uses arithmetic DROID rejects.

## Answer "what master does this target"

```bash
droid_tui analyze patch.ini --json | jq '.master'
```

`model` is `MASTER` or `MASTER18`; `source` says why:

- `declared` — the patch's own `# LABELS: master=16|18` comment decided it.
- `inferred` — exactly one model hosts every jack and circuit the patch uses.
- `default` — nothing constrains it, so it is drawn as MASTER.

`master.satisfiable == false` means the patch asks for things only MASTER18 can
do *and* things only MASTER can do; the validator reports both sides.

A `master_conflict` warning at `--deny error` is advisory — the Forge shows
these when you switch masters. It only fails the run under `--deny warning`.

## Check RAM headroom

```bash
droid_tui analyze patch.ini --json | jq '.ram'
```

`used − budget` is the headroom (negative). A negative value with no
`ram_overflow` finding means the patch resolved to the larger-budget model;
check `ram.model` before concluding anything.

## Find where the latency goes

```bash
droid_tui analyze patch.ini --json \
  | jq -r '.latency as $l | $l.worst[] | "\(.latency)\t\(.cable)\t\(.source) -> \(.sink)"'
```

`back_edges` counting more than a handful is the usual culprit: a cable whose
consumer runs before its producer costs almost a full loop.

## Reduce the latency and keep the result

```bash
droid_tui analyze big.ini --json | jq '.latency.avg'         # 4.0
droid_tui optimize big.ini --write big-latopt.ini            # save-as, never the source
droid_tui analyze big-latopt.ini --json | jq '.latency.avg'  # 1.0
```

Tune the search when the default is not enough:

```bash
droid_tui optimize big.ini --strategy minmax --weight 0.5 --write big-latopt.ini
```

`--weight` trades total latency (0.0) against the single worst edge (1.0).
`candidates[0]` is the best result; `--candidate 1` picks the next one if you
want to compare by hand. The emitted patch is a lossless reordering of the
source — only the section order changes.

To inspect before writing:

```bash
droid_tui optimize big.ini --stdout > /tmp/reordered.ini
diff <(grep '^\[' big.ini) <(grep '^\[' /tmp/reordered.ini)
```

## Analyze a patch you generated, without writing it to disk

```bash
cat > /tmp/gen.py <<'PY'
print("[button]")
print("  output = _TRIG")
PY
python3 /tmp/gen.py | droid_tui analyze - --json
```

`-` reads standard input. `patch.path` in the report reads `<stdin>`, and
`--write` is rejected for this input (there is no source to refuse or suffix);
use `--stdout` to emit a reordering.

## Explain a report that changed between two machines

```bash
droid_tui analyze patch.ini --json | jq '.schema'
```

`source`/`plugins` name the schema that answered. A plugin directory present on
one machine and not the other is the usual reason two reports disagree for one
patch; `--no-plugins` on both sides makes the comparison meaningful.
