# The JSON report contract

`droid_tui analyze … --json` prints one object. `droid_tui optimize … --json`
prints a different object that shares the `report_version`, `tool` and `patch`
fields. Both are printed pretty-printed, one document, followed by a newline.

`report_version` is the contract version: it changes only when a field's
meaning or presence changes. Bytes and counts are integers, latency values are
rounded to four decimals, and severity/label fields are lowercase strings.

## `analyze`

| field | type | meaning |
|---|---|---|
| `report_version` | integer | contract version (currently `1`) |
| `tool` | object | `{ "name", "version" }` of the binary that produced the report |
| `patch` | object | what was read — see [patch](#patch) |
| `schema` | object | which schema answered — see [schema](#schema) |
| `master` | object | the resolved master model — see [master](#master) |
| `ram` | object | RAM usage against the resolved model's budget — see [ram](#ram) |
| `validation` | array | schema/lint findings — see [validation](#validation) |
| `topology` | array | cable-topology findings — see [topology](#topology) |
| `latency` | object | forward-loop latency — see [latency](#latency) |
| `groups` | array | banner groups — see [groups](#groups) |

### patch

| field | type | meaning |
|---|---|---|
| `path` | string | the path as given, or `<stdin>` |
| `name` | string | the patch name (file stem) |
| `bytes` | integer | size of the source text |
| `sections` | integer | number of `[section]` blocks |
| `circuits` | integer | sections whose name resolves to a known circuit |
| `cables` | integer | distinct virtual cables |
| `hw_tokens` | integer | hardware components (buttons, pots, jacks, LEDs, …) |

### schema

| field | type | meaning |
|---|---|---|
| `circuits` | integer | circuits in the schema that answered |
| `source` | string | `embedded`, or `embedded+plugins` when plugin files were applied |
| `plugins` | array of strings | every applied plugin file, in load order |

Two runs that differ on the same patch are explained here first.

### master

| field | type | meaning |
|---|---|---|
| `model` | string | `MASTER` or `MASTER18` |
| `source` | string | `declared` (the patch says so), `inferred` (only one model hosts it), or `default` (nothing constrains it) |
| `satisfiable` | boolean | `false` when no master model can host the patch |

### ram

| field | type | meaning |
|---|---|---|
| `used` | integer | bytes the patch's circuits and controllers need |
| `budget` | integer | that budget for `model` |
| `model` | string | the model the budget belongs to |

### validation

One object per finding, in source order:

| field | type | meaning |
|---|---|---|
| `severity` | string | `error`, `warning` or `hint` |
| `code` | string | stable finding id, e.g. `unknown_param`, `missing_required`, `duplicate_cable`, `ram_overflow`, `master_conflict` |
| `message` | string | human text naming the section, parameter or cable |
| `line` | integer | 1-based source line |
| `col_start` | integer | 0-based byte column where the offending value starts |
| `col_end` | integer | 0-based byte column just past it |

An `error` here means the app would refuse to load the patch.

### topology

| field | type | meaning |
|---|---|---|
| `cable` | string | the virtual cable the finding is about |
| `severity` | string | `error` (two or more producers, `n → 1`) or `warning` (a sink with no producer) |
| `message` | string | human text |

### latency

Forward-loop latency: how far a consumer sits behind its producer in processing
order, in loop units. Empty graphs report zeros.

| field | type | meaning |
|---|---|---|
| `avg` | number | mean per-edge latency |
| `max` | number | largest single-edge latency |
| `back_edges` | integer | edges that wrap the loop boundary |
| `worst` | array | the slowest edges, worst first, at most ten |

`worst[]` entries: `cable` (string), `source` and `sink` (strings,
`"<circuit> <1-based instance>"` or a jack token), `latency` (number).

### groups

| field | type | meaning |
|---|---|---|
| `title` | string | the banner text, or `(unnamed)` for the leading implicit group |
| `start` | integer | first section index (0-based, inclusive) |
| `end` | integer | last section index (exclusive) |

## `optimize`

| field | type | meaning |
|---|---|---|
| `report_version` | integer | contract version |
| `tool` | object | `{ "name", "version" }` |
| `patch` | object | as in [patch](#patch) |
| `strategy` | string | the search that produced these candidates: `banner`, `global` or `minmax` |
| `weight` | number | the objective weight the search used: `0.0` is total latency, `1.0` is the worst edge |
| `baseline` | object | the current file order's latency — see [latency summary](#latency-summary) |
| `candidates` | array | proposed reorderings, best first — see [candidates](#candidates) |

### latency summary

`baseline`, and each candidate's `before`/`after`, carry `avg` (number), `max`
(number) and `back_edges` (integer), with the same meaning as in
[latency](#latency).

### candidates

| field | type | meaning |
|---|---|---|
| `label` | string | the strategy that produced it, e.g. `banner (default)` |
| `before` | object | latency summary of the patch as it is |
| `after` | object | latency summary of this ordering |
| `delta_avg` | number | `after.avg − before.avg` (negative is better) |
| `delta_max` | number | `after.max − before.max` (negative is better) |
| `order` | array of integers | a permutation: `order[i]` is the section index that belongs at file position `i` |
| `wrote` | string or null | the path `--write` actually wrote for this candidate (may be suffixed); `null` otherwise |

`candidates[0]` is the best by the objective that was searched.
