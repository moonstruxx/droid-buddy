# Design

## Context

See `proposal.md` — Why. The constraints that shape the approach:

- There is exactly one window-open path: `run` queues `GraphWindowRequest::Open`
  and `AppHandler::about_to_wait` acts on it (ADR 36). No runtime path opens or
  reopens a window, so "the startup open failed" and "the window never opened"
  describe the same condition.
- `AppHandler` already carries an unused `error: Option<color_eyre::Report>`
  field, and `run` already ends with `handler.error.map_or(Ok(()), Err)` — the
  plumbing for a fatal startup error exists but nothing ever sets it.
- `gui::destroy_window_for_panic` clears three thread-locals (`EGUI`, `DISPLAY`,
  `WINDOW`, in that order because the wgpu surface must die before its window)
  using `try_borrow_mut`, which guards a re-entrant *borrow* but not a
  **destroyed** thread-local (`LocalKey::with` panics once the TLS destructor
  has run), and does not guard a panic raised while the wgpu objects drop.
- `should_exit(close_requested, quit_requested)` already centralizes the quit
  decision as a pure function so it can be tested without an event loop
  (`main.rs`); the same technique applies to the two new verdicts.

## Goals / Non-Goals

**Goals:**

- Make "exit 0 without a window" structurally impossible, and make the reason
  of an unopenable window the process's error.
- Make the panic path unable to abort through a second panic.
- Keep both decisions in pure, unit-testable helpers rather than in event-loop
  code that tests cannot reach.

**Non-Goals:**

- No headless mode, no retry, no CLI flags, no change to the display stack.
- No change to normal start, input, rendering, pane or exit-on-quit behaviour.

## Decisions

### D1 — A failed startup open is fatal, and it is the run's error

`open_window` keeps its `[warn]` line but, when it fails, also sets
`handler.error` to a report naming the cause and calls `event_loop.exit()`;
`run` already converts `handler.error` into its `Result`. *Why*: the window is
the application (`native-rendering`), so a process that cannot open it has
nothing to keep running for; and the existing `error` field was built for this
but never written. *Alternative*: exit the process directly from the handler —
rejected, because it bypasses the `Result` contract and makes the failure
untestable.

### D2 — A run that never opened a window is an error, checked after the loop

`AppHandler` records `window_opened: bool` when `open` succeeds. After
`run_app`, `run` turns "not opened, no error" into an `Err` advisory. *Why*:
belt and braces for D1 — if some future path swallows an open failure again,
the process still cannot report success. It is also the exact regression the
issue describes.

### D3 — Teardown is idempotent and every step is contained

`destroy_window_for_panic` takes a one-shot `AtomicBool` (a second call returns
immediately) and performs each thread-local clear through a helper that wraps
the access in `std::panic::catch_unwind` (with `AssertUnwindSafe`), so a
`panic_access_error` from destroyed TLS — or a panic raised while a wgpu object
drops inside the clear — is swallowed and the remaining steps still run. *Why*:
a panic hook that can panic turns any failure into an abort, which is the
reported SIGABRT; `try_borrow_mut` alone cannot cover the destroyed-TLS case.
*Alternatives*: `std::mem::forget` the values — rejected, it leaks the surface
silently in a normal close/reopen; `catch_unwind` around the whole function
only — rejected, one panic would skip the remaining drops and the window would
be left orphaned.

### D4 — Both verdicts are pure helpers

`startup_open_verdict(open_error: Option<String>) -> Result<(), String>` (the
startup open outcome) and
`completion_verdict(window_opened: bool, open_error: Option<&str>) -> Result<(), String>`
(the post-loop check), plus `teardown_step(label, f)` (the contained-step
helper), are free functions with unit tests, mirroring `should_exit`. *Why*:
the event loop cannot be built in a unit test, and the defect is a decision,
not a mechanism.

## Risks / Trade-offs

- **A window that opens then dies will now exit instead of lingering** → that
  is the intended contract; `close`/`q` paths are unchanged and still exit
  cleanly.
- **`catch_unwind` can hide a genuine bug in teardown** → the helper reports
  the swallowed panic on stderr when a backtrace-friendly `RUST_BACKTRACE` is
  set, so it is contained and diagnosable rather than silent.
- **The one-shot guard makes a legitimate second teardown a no-op** → there is
  no second window (ADR 36); if one ever returns, the guard must be reset in
  `GraphWindow::open` and the regression test states that.
- **The original symptom does not reproduce on HEAD** → recorded in the
  proposal; the change is justified by the two defects that can still produce
  it, and the report will say so explicitly.
