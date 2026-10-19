# Design: essential params satisfied by documented alternatives

## Context

`missing_required` currently reads:

```rust
let is_ranged = input.prefix.is_some() && input.count.is_some();
if input.essential == 2 && !is_ranged {
    // every expanded name must be in `seen`
}
```

`seen` is an exact-key map of the entries the patch wrote in that section, so
membership is reliable: the `[compare] ifequal` findings are genuine omissions,
not a keying bug (the patch contains no `ifequal` at all).

The schema is the authority for *what exists*, but `essential` is a
one-dimensional flag; it cannot express "A or B". The manual does, in prose.

## Decision

Satisfy an `essential: 2` param by an explicit, documented alternatives table
kept in `validation.rs` (beside `JACK_RULES`), not by schema metadata and not by
a generic heuristic.

Table entries:

| circuit | required param | satisfied by | evidence |
| --- | --- | --- | --- |
| `lfo` | `hz` | `taptempo`, `rate` | manual: `hz` cannot be used *with* `taptempo`; both combine with `rate`; `hz` also defaults to 1 |
| `gatetool` | `inputgate` | `inputtrigger`, `inputedge` | manual: "three different types of inputs. Usually you would patch only one of these" |
| `contour` | `gate` | `trigger` | manual: "two alternative inputs: `gate` and `trigger`" |
| `compare` | `ifequal` | `ifgreater`, `ifless`, `else` | manual: "It is allowed to omit any of the inputs `ifless`, `ifequal`, `ifgreater` or `else`" |

Rule: for a param with an entry, skip the finding when **at least one**
alternative name is present in `seen` for the same section. Without an entry, or
with an entry and no alternative present, behaviour is byte-identical to today.

## Why not the alternatives

**Upstream schema metadata (`circuits.json` gains `alternative: [...]`).**
The cleanest long-term shape, and it would fix `droid-lsp` at the same time. But
it lives in a vendored submodule (`ext/droid-lsp`), requires a matching
`diagnostics.ts` change to stay in parity, and would have to be carried across
every submodule bump. Out of scope for this repo's fix, and a poor trade for a
4-entry policy. Recommended as a follow-up upstream report.

**A generic "has a default ⇒ optional" rule.** Measured: 43 of the 72 singular
`essential == 2` inputs declare a default other than `unpatched`, and 47 declare
some default at all, `[compare] input` (`default: "0"`) among them, which is
genuinely required. Such a rule would silence 65 % of required-param checking and
hide real defects. Rejected.

**Dropping `essential` handling for the four circuits entirely.** Would silence
e.g. a `[gatetool]` with none of the three inputs, or a `[compare]` with no
branch at all. The table is narrower: it needs positive evidence of the
alternative.

## False-positive risk

An alternative must not silence a genuinely missing param. Guardrails:

1. The alternative must be written in the **same section** (consume `seen` only,
   never the whole patch).
2. Only the exact param named in the table is exempted; siblings keep warning.
   `[compare]` with `ifgreater` present still warns about a missing `input`.
3. No entry may list an alternative that is itself `essential: 2` without its own
   entry — otherwise two params could each excuse the other while both are absent.
   Checked once in a unit test over the table.
4. No entry is inferred; every row cites manual prose. A circuit absent from the
   table keeps the current behaviour, which is what pins the regression corpus.

Residual, deliberate: the manual says a `[compare]` with *all four* branches
omitted is legal (`ifequal` then defaults to 1). Under this design such a section
still warns, because none of the alternatives is present. That is accepted:
a compare with no branch at all is at least suspicious, the corpus never does it,
and suppressing it would remove the only remaining check on that circuit. The
design note is recorded here so a future reader does not "fix" it by accident.

## The `compare` mechanism, stated plainly

No set-detection bug exists. Every one of the six `[compare]` sections writes
`input`, `compare` and `output`; none writes `ifequal`. `seen` records the keys
verbatim and compares verbatim, and all six sections also produce no
`unknown_param`/`duplicate_param` findings, which is the observable proof that
`seen` is correct for these sections. The warning was policy, not parsing
(issue text: "although the patch sets it" — that part is wrong).

## Tests

- One unit test per table row, positive and negative:
  `[lfo]` with `taptempo` only → no `hz` finding; `[lfo]` with neither
  `taptempo` nor `rate` → `hz` finding. Same shape for the other three rows.
- `[compare]` with `ifgreater` but no `input` → still reports `input`.
- Table sanity: every old/new name resolves to a declared param of that circuit,
  and no row creates a mutual-exclusion cycle.
- Corpus regression on a self-contained fixture: `fixtures/validation/essential_alternatives.ini`
  carries one section per row plus a control section that sets none of the
  alternatives — the first four must produce no `missing_required`, the control
  exactly one. The 14 → 0 measurement on `patches_remote/_droid_mpfs5drum.ini`
  stays evidence in `proposal.md` and is not a test dependency: that path is a
  symlink into a working tree outside the repo, so CI cannot rely on it.
