# Change: panel-rendering-fixes

## Why

Eight ready beads issues (observed on `droid_mpfs5melody2.ini`) report panel-rendering defects: boxed LED cells garble at narrow widths (`droid_tui-wsu`), the status bar duplicates the Scale/Orientation segments (`droid_tui-rma`), the file picker shows the parent directory's plain name instead of `..` (`droid_tui-8zw`), the P2B8 panel has uneven vertical row spacing (`droid_tui-irf`), over-long labels hard-cut without an ellipsis (`droid_tui-lsd`), and LED-joined boxed rendering only benefits buttons instead of all control kinds (`droid_tui-8kr`, extends ADR 10). `droid_tui-0yb` ("Controller 3 support") is covered by the same fixes: the `# CONTROLLER 3:` (B3.*) panel is where the garbled cells, uneven spacing, truncated labels, and unjoined LED cells were observed. `droid_tui-tlh` ("Joined LEDs and Buttons") is superseded by `droid_tui-8kr`. `droid_tui-nnq` (NN/ML outlier detection) is a parked roadmap item and out of scope.

## What Changes

- **Narrow-width boxed-cell fallback** (`src/ui.rs`): boxed LED-associated cells never emit partial border fragments when the available cell width is smaller than the box content — either the content shrinks to fit inside a complete box or the cell falls back to unboxed two-line rendering. (`droid_tui-wsu`)
- **Status bar segment dedup** (`src/ui.rs`): the bottom status bar composes each segment (Scale, Orientation, …) exactly once. (`droid_tui-rma`)
- **Picker parent entry** (`src/app.rs`, `src/ui.rs`): the parent-directory entry renders as `..` (first entry when not at filesystem root), Enter navigates up without closing, no `..` entry at root; `is_entry_selectable`'s `..` branch becomes live; entries sort dirs-first. (`droid_tui-8zw`)
- **Even panel row spacing** (`src/ui.rs`): consistent vertical rhythm between same-kind component rows in a panel (boxed-vs-unboxed height differences no longer create irregular gaps). (`droid_tui-irf`)
- **Label ellipsis** (`src/ui.rs`): over-long labels truncate with a trailing `…` while hit rects and alignment stay unchanged. (`droid_tui-lsd`)
- **Joined boxes for all control kinds** (`src/patch.rs`, `src/ui.rs`): LED-association detection extends the bare-`led`/suffix-paired `ledN` rule beyond button/pot to every element param family (encoder/switch/fader + any schema `ledN` group), and the boxed renderer displays each kind's state inside the box (knob/encoder percentage, switch glyph); associated LEDs never render standalone. (`droid_tui-8kr`, after `droid_tui-wsu`)

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `controller-panels`: boxed cells fall back cleanly at narrow widths; joined-box rendering covers all control kinds with a resolvable LED association; uniform row rhythm and ellipsized labels.
- `file-picker`: parent entry labeled `..` with live Enter-up navigation.
- `mouse-interaction` (status): status bar segments composed exactly once.

## Impact

- Affected specs: `controller-panels`, `file-picker` (delta)
- Affected code: `src/ui.rs`, `src/app.rs`, `src/patch.rs`, `src/regression.rs`, fixtures/snapshots/gallery scenarios
- Baseline: full suite stays green; `cargo insta test --check` remains the strict gate.

## Non-goals

- `droid_tui-nnq` (NN/ML UI-outlier detection) stays parked and open.
- The in-progress `latency-optimizer-upgrades` change and its branch/worktree are untouched.
- No `.ini` mutation, no hardware bridge, no network.

## Beads issues

| Issue | Status | Covered by |
|---|---|---|
| droid_tui-wsu | closed | task 1.1 |
| droid_tui-rma | closed | task 1.2 |
| droid_tui-8zw | closed | task 1.3 |
| droid_tui-irf | closed | task 1.4 |
| droid_tui-lsd | closed | task 1.5 |
| droid_tui-8kr | closed | tasks 2.1 + 2.2 |
| droid_tui-0yb | closed | tasks 1.1/1.4/1.5/2.1/2.2 (Controller 3 panel) |
| droid_tui-tlh | closed (superseded) | supersedes into droid_tui-8kr |
| droid_tui-nnq | stays open | parked roadmap item |