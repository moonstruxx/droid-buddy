# Proposal: essential params satisfied by documented alternatives

## Why

`missing_required` (see `src/validation.rs`, the `essential == 2` arm) treats every
non-ranged `essential: 2` input as mandatory. For several circuits the firmware and
the DROID manual document a *choice* of inputs instead, so a correct patch is
reported as defective.

Measured on the real patch family, `patches_remote/_droid_mpfs5drum.ini`
(311 sections, 307 known circuits), against the embedded schema:

- `validate_patch` yields exactly **14 findings, all `missing_required`, 0 other codes**.
- Breakdown:
  - `[compare] ifequal` × 6 (lines 2646, 2679, 2687, 2747, 2834, 2906)
  - `[contour] gate` × 4 (lines 2697, 2794, 2867, 2939)
  - `[gatetool] inputgate` × 3 (lines 210, 3170, 3259)
  - `[lfo] hz` × 1 (line 327)

Status of the three claims in `droid_tui-0yy` after checking the data:

1. **`[lfo] hz` — CONFIRMED (with a nuance).** `hz` is `essential: 2`, but a
   taptempo-driven LFO does not need it, and `hz` additionally declares
   `default: "1"`. The manual description is self-contradictory: it says
   "you cannot use `hz` at that same time as `taptempo`" and then shows a patch
   combining both (`taptempo = G1` + `hz = 3`, "three times the tap tempo").
   Either way, omission is legal.
2. **`[gatetool] inputgate` — CONFIRMED.** The manual opens with "gatetool has
   three different types of inputs. Usually you would patch only one of these"
   and then lists `inputgate`, `inputtrigger`, `inputedge`. The three drum
   sections patch `inputtrigger` or `inputedge` only.
3. **`[compare] ifequal` — the framing is REFUTED, the conclusion still holds.**
   The drum patch does **not** set `ifequal` anywhere (zero occurrences in the
   file); this is not a set-detection bug. `seen` is keyed by the exact patch key
   and `input`/`compare`/`output` are all correctly seen in all six sections.
   The real basis is the manual: "It is allowed to omit any of the inputs
   `ifless`, `ifequal`, `ifgreater` or `else`."

A fourth family not mentioned in the issue was found: `[contour] gate` × 4. The
manual documents `gate` and `trigger` as "two alternative inputs" for starting the
envelope; the drum patch drives all four contours from `trigger`.

Upstream has no metadata to lean on: `ext/droid-lsp/droid-lsp/src/circuits.json`
carries only `essential` and `essential_count` (no exclusivity/alternative/conflict
keys), and `diagnostics.ts` (lines 202-227) implements exactly the same
`essential === 2 && !(prefix && count)` policy.

## What changes

`missing_required` consults a small, documented alternatives table before emitting
a finding: when an `essential: 2` param is absent but one of its documented
alternatives is present in the same section, the param is regarded as satisfied.
Where no alternative is present, the warning is unchanged.

## Capabilities

The validator is specified by the `patch-validation` capability, not
`patch-parsing`: `openspec list --specs` shows a distinct `patch-validation`
spec, and its requirement "Missing required parameter is a warning at header" is
the one this change narrows. `ARCHITECTURE.md` §3.10 ("Patch Validation
(`src/validation.rs`)") confirms the ownership.

- Modify **`patch-validation`**: the `missing_required` rule gains the
  documented-alternatives exception.

## Impact

- `src/validation.rs`: one table plus one predicate in the `essential == 2` arm.
- Tests: unit tests per case plus a corpus regression pinned to the drum patch.
- `ARCHITECTURE.md` §3.10: one sentence describing the exception.
- No schema edit, no `ext/droid-lsp` submodule edit, no severity change.

## Non-goals

- Changing `circuits.json` or `diagnostics.ts` (vendored upstream). The upstream
  inconsistency is worth reporting, but fixing it is a separate, cross-repo task.
- A generic "an `essential: 2` param with a default is optional" rule. 43 of the
  72 singular `essential == 2` inputs declare a non-null default (including
  `[compare] input`, which is genuinely required), so that rule would silence
  real findings.
- Suppressing `missing_required` entirely, or downgrading it from `Warning`.
- Ranged (`prefix` + `count`) params: already skipped, unchanged.
- Any other validation code (`unknown_param`, `duplicate_param`, cables, RAM).
