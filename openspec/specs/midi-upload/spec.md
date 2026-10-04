# midi-upload Specification

## Purpose

Send the in-memory patch to DROID hardware over MIDI SysEx (USB-MIDI or DIN MIDI), confirmed and in the background, so edits are heard without leaving the app.

## Requirements

### Requirement: Build the SysEx payload from memory

The app SHALL serialize the in-memory patch to 7-bit-clean text and frame it as `F0 00 66 66 50 … F7`, byte-identical in framing to the `droidpatch` reference for the same input text.

#### Scenario: Payload matches the reference framing

- **WHEN** the in-memory patch is built for upload
- **THEN** the bytes start with `F0 00 66 66 50`, end with `F7`, and contain no comment, whitespace-padding, or non-ASCII bytes outside quoted strings.

### Requirement: Confirm before sending

`U` SHALL open a confirm modal showing byte count and transport; `y` sends, `n`/`Esc` cancels without side effects.

#### Scenario: Cancelled send touches nothing

- **WHEN** the user presses `n` or `Esc` in the confirm modal
- **THEN** no process spawns and the rack keeps running its current patch.

### Requirement: Background send with progress and verdict

The send SHALL run without freezing the loop, show waiting-bar progress, and end in a status verdict (success byte count, failure reason, or abort on quit).

#### Scenario: Slow transfer stays interactive

- **WHEN** a send is in flight
- **THEN** keys, panes, and paints keep working until the verdict lands.

### Requirement: Both transports, explicit failures

USB-MIDI (X7 autodetect) and DIN MIDI SHALL both be reachable; missing tools or hardware SHALL surface as a status error naming what's absent.

#### Scenario: No hardware connected

- **WHEN** no DROID port is detected
- **THEN** the modal offers no `y` path and the status names the missing hardware or tool.
