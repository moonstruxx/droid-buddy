# Keybinding

## Overview

Vim-style prefix key system for the source viewer. The `g` key arms a prefix
mode with a lazy timeout; `g` + `v` opens the source viewer. While the viewer
is open, dedicated keys control scrolling, jumping, and closing.

## Requirements

- REQ-1: Pressing `g` arms the prefix mode (`app.prefix = Some(PrefixState { started: Instant::now() })`). No timer thread is spawned.
- REQ-2: Prefix timeout is 1 second, checked lazily when the next event arrives. If expired, the prefix is cleared and the new key processes normally.
- REQ-3: `g` + `v` while prefix is armed sets `app.showing_viewer = true`, clears the prefix, and populates `app.viewer_patch` from the current patch's circuits.
- REQ-4: While viewer is open: `j` or `Down` scrolls main area down (increments `app.viewer_scroll`), `k` or `Up` scrolls up (decrements, saturating at 0).
- REQ-5: While viewer is open: `Enter` jumps the main area scroll to the selected sidebar circuit's position.
- REQ-6: While viewer is open: `Esc` closes the viewer (`app.showing_viewer = false`).
- REQ-7: `Esc` while prefix is armed cancels the prefix without other side effects. Does not clear the active shift group.
- REQ-8: Any other key while prefix is armed: cancels the prefix and processes the key normally (as if prefix was never armed).

## Design Decisions

- Decision 1: Lazy timeout check (no background timer). Rationale: the app is event-driven; checking expiry on the next keypress avoids threading complexity and keeps the event loop simple. A stale prefix that nobody presses is harmless.
- Decision 2: `Esc` cancels prefix without clearing shift group. Rationale: shift group activation is an independent concern from prefix mode. Cancelling a mistaken `g` press should not disturb an active shift view.
- Decision 3: `g` + `v` chosen for viewer (not `g` + `s` or `g` + `p`). Rationale: `v` is mnemonic for "viewer" and avoids collision with potential future `g` + `s` (save/search) bindings.
- Decision 4: Scroll uses `u16` saturating arithmetic. Rationale: prevents underflow on decrement at 0, avoids panic on overflow at max.

## Scenarios

### Scenario: Open viewer with prefix
Given no prefix is armed
And a patch is loaded
When user presses `g` then `v`
Then `app.showing_viewer` becomes true
And `app.viewer_patch` contains the patch's circuits
And `app.prefix` is `None`

### Scenario: Prefix timeout expires
Given prefix was armed 2 seconds ago
When user presses any key
Then the prefix is cleared
And the key processes normally (not as a prefix combo)

### Scenario: Cancel prefix with Escape
Given prefix is armed
And shift group 3 is active
When user presses `Esc`
Then `app.prefix` becomes `None`
And `app.active_shift` remains `Some(Group3)`

### Scenario: Non-matching key while prefix armed
Given prefix is armed
When user presses `h`
Then the prefix is cleared
And `h` processes as a normal key event

### Scenario: Scroll viewer
Given viewer is open with scroll at 0
When user presses `k` (up)
Then `app.viewer_scroll` remains 0 (saturating)
When user presses `j` (down) three times
Then `app.viewer_scroll` is 3
