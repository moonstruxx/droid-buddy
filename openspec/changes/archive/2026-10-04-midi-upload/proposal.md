# Proposal

## Why

Iterating on a hand-edited `.ini` currently round-trips through a terminal
and the `droidpatch` script (or the Forge app). The TUI already owns the
patch in memory — including optimizer reorderings the disk file does not
have — so sending it straight to the rack from the app closes the loop:
edit, inspect, upload, hear it immediately.

## What Changes

- A pure SysEx builder (`src/sysex.rs`, no process/UI dependency): the
  in-memory patch → 7-bit-clean text (comment/whitespace/non-ASCII stripping
  ported from `droidpatch`) → `F0 00 66 66 50 … F7` framing.
- `U` (Shift+u) opens a confirm modal (byte count + transport) for the
  in-memory patch; `y` sends, `n`/`Esc` cancels.
- The send shells out to `amidi`/`sendmidi` in the background with a
  waiting-bar progress state; the loop stays alive and a status verdict
  (success byte count or named failure) lands on completion.
- Transports: USB-MIDI (X7 autodetect via `amidi -l`) and DIN MIDI;
  absent hardware or missing tools → status error naming what's missing,
  never a silent no-op.

## Capabilities

### New Capabilities

- `midi-upload`: in-memory patch upload to DROID hardware over MIDI SysEx.

### Modified Capabilities

- `keybinding`: `U` upload chord, confirm-modal `y`/`n`, help rows.

## Impact

- Touched code: new `src/sysex.rs`, `src/app.rs` (send state/progress),
  `src/handler.rs` (`U`/confirm routing), `src/gui/overlays.rs` (confirm
  modal + waiting bar), `src/help.rs` (rows).
- No new dependencies (shell-out edge, no `midir`); no config keys; no
  persistence; no `.ini` mutation.
- Declared scope expansion: the app becomes a patch sender, not just a
  viewer (ARCHITECTURE.md §6/§15 still say otherwise until regenerated).

## Non-goals

- No native MIDI stack (`midir`/ALSA/JACK) unless the tool dependency bites.
- No firmware update, calibration, or SD-card flows.
- No receive path (rack → app); upload only.
- No ARCHITECTURE.md/DESIGN.md hand-edits (derived artifacts).

## New types (YAGNI consumer check)

- `SysexPayload` (framed bytes + transport hint, in new `src/sysex.rs`):
  consumed by the `App` send state and the confirm modal; no other new
  types planned. Send progress is scalar state on `App`.
