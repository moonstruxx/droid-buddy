# Herdr Integration Specification


## Purpose

Two modes for launching the source viewer in a separate terminal pane or
window. Mode 1 uses Herdr pane splitting when `HERDR_ENV=1` is set. Mode 2
falls back to detecting the user's terminal emulator and spawning a new
window with the appropriate CLI flags.

## Requirements

- REQ-1: Mode 1 activates when environment variable `HERDR_ENV` is set to `1`.
- REQ-2: Mode 1 sequence: run `herdr pane split --current --direction right --cwd $PWD --no-focus`, parse JSON output for the new pane ID, then run `herdr pane run <pane_id> "droid_tui --view-source"`.
- REQ-3: Mode 2 activates when `HERDR_ENV` is not set or Mode 1 fails.
- REQ-4: Mode 2 detects terminal from `$TERM` environment variable and tries launchers in order: kitty (`kitty @ new-window --offscreen droid_tui --view-source`), xterm (`xterm -e droid_tui --view-source`), gnome-terminal (`gnome-terminal -- droid_tui --view-source`), alacritty (`alacritty -e droid_tui --view-source`).
- REQ-5: Mode 2 tries the `$TERM`-matched terminal first, then falls through remaining launchers in order.
- REQ-6: On launch failure (no terminal detected, command fails), display a graceful status message. `app.viewer_mode` remains unchanged.
- REQ-7: `app.viewer_mode` is set to `Herdr` after successful Mode 1 launch, `Fallback` after successful Mode 2 launch.

## Design Decisions

- Decision 1: Herdr checked first, fallback second. Rationale: Herdr provides a controlled pane split within the existing terminal session, which is the preferred UX. Fallback is for standalone terminal use.
- Decision 2: `$TERM`-matched terminal tried first in fallback mode. Rationale: if the user is running kitty, `kitty @` is the native way to open windows from within kitty and will work reliably. Trying the "wrong" terminal's CLI first would waste time and may produce confusing errors.
- Decision 3: `--view-source` CLI flag is the contract between the launcher and the launched instance. Rationale: the new `droid_tui` process needs to know it should start in viewer mode rather than normal mode.
- Decision 4: Failure is non-fatal — status message only, no panic. Rationale: the user can still view the source by other means; a failed pane split should not crash the running TUI.

## Known Gaps

- `--view-source` CLI flag is not yet parsed by the `droid_tui` binary. The launch commands will start a new instance in normal mode until this flag is implemented. This is a deferred follow-up tracked separately.

## Scenarios

### Scenario: Herdr pane split succeeds
Given `HERDR_ENV=1` is set
And `herdr` is available on PATH
When user triggers viewer launch
Then a right-side pane opens running `droid_tui --view-source`
And `app.viewer_mode` is `Herdr`

### Scenario: Herdr unavailable, kitty detecteded
Given `HERDR_ENV` is not set
And `$TERM` is `xterm-kitty`
When user triggers viewer launch
Then `kitty @ new-window --offscreen droid_tui --view-source` runs
And `app.viewer_mode` is `Fallback`

### Scenario: No terminal detected
Given `HERDR_ENV` is not set
And `$TERM` does not match any known terminal
And all fallback launchers fail
When user triggers viewer launch
Then a status message reports the failure
And `app.viewer_mode` remains `None`

### Scenario: Herdr fails, fallback succeeds
Given `HERDR_ENV=1` is set
And `herdr` command fails
And `$TERM` indicates gnome-terminal
When user triggers viewer launch
Then Mode 1 fails gracefully
And Mode 2 launches via `gnome-terminal -- droid_tui --view-source`
And `app.viewer_mode` is `Fallback`
