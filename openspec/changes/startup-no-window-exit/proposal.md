# Proposal

## Why

Bead `droid_tui-533`: since 2026-10-04 the app has been reported to exit 0 in
about 0.16 s with no window and no output, and the debug build aborts with a
double panic during TLS teardown (`main.rs:23` → `gui/mod.rs:1011`
`destroy_window_for_panic` → `TLS panic_access_error`, plus a wgpu `LockTrace`
panic while the `Queue` drops during unwinding).

Two defects in the startup and teardown paths can produce exactly those
symptoms, and both are real on the current code:

- **A window that never opens is not an error.** `AppHandler::open_window`
  prints `[warn] graph window open failed (…)` and keeps running, and `run`
  returns `Ok(())` whenever the event loop returns without a window. A launch
  that cannot create its window therefore either spins with no window or exits
  **successfully** with the reason buried on stderr — exit 0 without a window
  is exactly the reported symptom class, and it is indistinguishable from a
  clean quit for a supervisor, a script, or a user.
- **The panic hook is not teardown-safe.** `destroy_window_for_panic` guards
  re-entrant *borrows* (`try_borrow_mut`) but not a **destroyed** thread-local:
  once the thread's TLS destructors have run, `LocalKey::with` panics, and the
  hook dropping the wgpu objects can itself panic while the process is
  unwinding — a double panic aborts. That is the reported SIGABRT.

On HEAD the original symptom does not reproduce: the release binary keeps
running with a window in this environment (`timeout 10` → 124), and a launch
with no display already fails cleanly through `EventLoop::new()` (exit 1 with
a `failed to create winit event loop` report). The fix is therefore scoped to
the two defects that remain reachable, so the class cannot come back.

## What Changes

- **A launch that cannot open its window fails loudly.** The startup open
  failure is recorded and ends the event loop with the error; `run` returns it,
  so the process exits non-zero with a diagnostic naming the cause instead of a
  silent success.
- **No window, no success.** If the event loop returns and no window was ever
  opened, `run` returns an error stating that, closing the exit-0-with-no-window
  hole even if an open failure is ever swallowed again.
- **Teardown can never double-panic.** `destroy_window_for_panic` becomes
  idempotent (a one-shot guard, so a second call is a no-op) and every
  thread-local access it performs is wrapped so a panic — including
  `panic_access_error` from already-destroyed TLS and a panic while a wgpu
  object drops — is swallowed. The panic hook still chains to the previous
  hook.
- **Regression tests** pin the exit verdicts (window opened / not opened / open
  failed) and the teardown guard (two calls are safe; a panicking teardown step
  is swallowed).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `native-rendering`: the application's lifecycle contract gains an exit
  contract (a launch that cannot open its window exits non-zero with the
  cause; a run that never opened a window is not a success) and a new
  requirement that panic teardown is total — idempotent and unable to panic
  while unwinding.

## Impact

- Touched: `src/main.rs` (the startup open path, the run verdict, and the new
  pure helpers), `src/gui/mod.rs` (`destroy_window_for_panic`), `src/main.rs`
  and `src/gui/mod.rs` test modules, and the `native-rendering` spec.
- Behaviour the user sees: with a working display, nothing — the window opens
  as before. Without one, the process now exits non-zero with the reason
  instead of succeeding silently; a panic still closes the window but can no
  longer abort the process through a second panic.
- No config key, no persistence, no dependency, no crate feature, no change to
  the rendering, input, or pane behaviour.
- Review point: the exit contract is the visible half; verify the teardown
  change by reading that every TLS access in the function is inside the
  guard.

## Non-goals

- No fix to the host display stack (the degraded X :0 of the report) and no
  headless runtime mode — `native-rendering` explicitly excludes one.
- No retry/reopen of a failed window (there is one window-open path, at
  startup, ADR 36), no window-open timeout, no new CLI flags.
- No change to `EventLoop::new()` failure handling, which already reports and
  exits 1.
- No rework of the wgpu/egui drop order beyond making it panic-safe.
