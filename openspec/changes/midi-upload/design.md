# Design

## Context

See proposal.md (Why). Current state: the app holds the patch in memory
(`App.patch`, serializable via the lossless `write_to_ini`/`render_ini`
writer); the reference upload is the `droidpatch` bash script (strip with
`sed`/`tr`, wrap `F0 00 66 66 50 … F7`, send with `amidi -p <port> -s` or
`sendmidi dev droid syf`); the event loop is single-threaded with no async
runtime; a 50 KB transfer at 31250 baud takes ~15–20 s.

## Goals / Non-Goals

**Goals:**

- Upload exactly what's in memory, confirmed, in the background, with
  visible progress and an unambiguous verdict.
- Both USB-MIDI and DIN MIDI reachable; failures name the missing piece.

**Non-Goals:**

- Native MIDI, firmware/SD flows, rack-to-app transfer.

## Decisions

- **Memory, not disk** (over file semantics): serialize `App.patch` via the
  lossless writer, so optimizer previews upload as-previewed. Rationale:
  the user's explicit call; the confirm modal shows the byte count so the
  surprise is visible. Alternative (disk file) rejected per feedback.
- **Shell-out edge** (over `midir`): `amidi`/`sendmidi` do transport.
  Rationale: zero new deps, ~100 lines, keeps the no-hardware-bridge
  posture honest (prepare in-app, deliver via system MIDI).
- **Forked send + indeterminate waiting bar** (over blocking): spawn the
  child, poll per frame, status shows elapsed + spinner. Rationale: MIDI
  gives no progress bytes through `amidi`; a determinate bar would be
  theater. The loop never freezes.
- **`U` (Shift+u)** (over bare `u`): weighty global action follows the
  `Shift+c`/`Shift+r` precedent; plain-letter space near patch keys is
  crowded and `u` risks collisions.
- **Confirm modal with transport + bytes** (over direct send): the rack
  switches patches instantly — destructive weight, `y`/`n` gate.

## Risks / Trade-offs

- [Risk] 15–20 s background send outlives user attention; quit mid-send →
  Mitigation: killing the child on quit; verdict never lands on a dead
  transfer, status notes the abort.
- [Risk] `amidi`/`sendmidi` absent or no DROID port →
  Mitigation: preflight probe before the modal confirms transport; modal
  lists what's missing instead of offering `y`.
- [Risk] In-memory/uploaded divergence confusion (disk ≠ rack) →
  Mitigation: verdict status states the source ("sent in-memory patch,
  N bytes"); saving to disk stays a separate act.
