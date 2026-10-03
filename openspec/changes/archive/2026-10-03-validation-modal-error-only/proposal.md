# Proposal

## Why

Two problems in one area. First, `openspec/specs/patch-validation/spec.md` states that patches with only `Warning`/`Hint` findings *"SHALL load but also open the modal"*, but the code has never done that: both load paths (`App::load_patch`, `App::load_patch_at`) set `showing_validation = false` for a no-Error load. The written contract and the executable spec disagree, and have since the original validation change.

Second, `App::set_validation` — the one function that *does* open the modal for any non-empty finding list — has no callers at all. It is dead code that reads like the live rule, and it is what led bead `droid_tui-6gy` to report a modal that auto-opens on load and swallows the `g g` chord. That report does not reproduce: loading `fixtures/droid_mpfs5melody2.ini` (2121 Warning/Hint findings, zero Errors) leaves `showing_validation = false` through both load paths.

The real, reproducible gap is discoverability: a warnings-only load reports only `Loaded <name>`, so its findings stay invisible until the user happens to press `e`.

## What Changes

- **Spec** (`patch-validation`): the auto-open rule is restated to match the code — a patch with at least one `Error` fails to load and opens the modal; a patch with only `Warning`/`Hint` loads **without** opening the modal and reports its finding count in the status bar with a hint that `e` opens the list.
- **Code**: both load paths add that status hint for a Warning/Hint-only load; `App::set_validation` is deleted (no consumer — the YAGNI anchor applies); `e` keeps opening the modal on demand.
- **Regression**: a warnings-only load leaves the modal closed, the status names the count with the `e` hint, `e` opens the modal, and `g g` still opens the graph.
- Bead `droid_tui-6gy` is closed with this evidence (not reproducible as reported; the real defect was the missing hint plus the spec drift).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `patch-validation`: the "Fail load on Error, modal error list with navigation" requirement no longer auto-opens the modal for Warning/Hint-only loads; it requires a status-bar hint that names the finding count and the `e` key.

## Non-goals

- No change to the validator's checks, severities, spans, or ordering.
- No change to the Error gating path or its `Load failed: N error(s) — press 'e' to view` status.
- No redesign of the validation modal, its navigation, or its rendering.
- No change to the `keybinding` spec's `e` toggle.
- No new configuration or persistence.

## Impact

**Affected code:**

- `src/app.rs` — the two load paths' status hint; `set_validation` deleted
- `src/regression.rs` — the regression

**Affected spec:** `openspec/specs/patch-validation/spec.md`

**No schema, dependency, data-format, or config changes.**
