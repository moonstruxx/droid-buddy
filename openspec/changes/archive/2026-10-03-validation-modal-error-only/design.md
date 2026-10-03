# Design

## Context

See `proposal.md` — Why. Both `App::load_patch` and `App::load_patch_at` resolve three load outcomes:

1. **Gated Error** (hard error and a patch is already loaded): the previous patch is kept, `showing_validation = true`, status `Load failed: N error(s) — press 'e' to view`, returns `false`.
2. **First-load Error**: the patch is installed anyway so fixtures bootstrap, `showing_validation = false` (the code comments "Do not auto-show modal for first-load error either; keep snapshots stable"), status `Loaded <name>`.
3. **No Error** (Warning/Hint only, or clean): the patch is installed, `showing_validation = false`, status `Loaded <name>`.

So the modal is already error-only in effect. The spec's sentence about Warning/Hint "also open the modal" describes a rule the code never implemented, and `App::set_validation` — the only function that opens the modal for any non-empty list — is unreferenced.

## Goals / Non-Goals

**Goals:**

- Make the written contract and the executable behavior agree.
- Make a Warning/Hint-only load's findings discoverable without opening a modal.
- Remove the dead function that misrepresents the rule.

**Non-Goals:**

- Changing which findings gate a load, or their severity classification.
- Changing the modal's content, navigation, or rendering.

## Decisions

**D1 — Fix the spec to the code, not the code to the spec.** The code's behavior is the one users want (a full-screen modal that eats app chords on a 2121-finding warnings-only patch is hostile) and the one every existing test and snapshot already assumes. *Alternative considered:* make the code auto-open the modal as the spec says — rejected, it reintroduces exactly the symptom the bead reported.

**D2 — Surface the count in the status bar.** A Warning/Hint-only load gets a status line naming the count and the `e` key, matching the Error path's existing `press 'e' to view` phrasing. This is the smallest change that makes the findings discoverable while keeping the modal on demand. *Alternative considered:* a transient toast or one-shot status — rejected as a new surface with no precedent here.

**D3 — Delete `App::set_validation` rather than repair it.** It has no callers, and it is why the bead's author read the rule as "opens for ANY finding". Leaving it — even corrected — keeps a second, unused implementation of the same policy. *Alternative considered:* keep it and have both load paths call it — rejected as a larger refactor with no behavioral gain, since the load paths already inline the logic alongside their gating branches.

## Risks / Trade-offs

| Risk | Mitigation |
|---|---|
| A caller of `set_validation` exists outside `src/` (integration test, example, doc) | Verified zero references repo-wide before proposing; the task re-verifies with a reference sweep before deleting. |
| The hint could be mistaken for an error | It is emitted only for a no-Error load; the Error path keeps its distinct `Load failed:` wording. |
| Warning/Hint counts can be large (melody2: 2121), so a naive message is unwieldy | The hint reports the count, not the findings; the modal lists them sorted by `(line,col)` when `e` is pressed. |
| The first-load-Error path stays modal-less | Out of scope by D1: that behavior is deliberate (fixture bootstrapping) and already carries a status message. |
