//! Profiling gate: runs core pure phases over the scale-anchor workload under
//! named wall-clock budgets so a CPU hog or endless loop fails the suite with
//! the phase name and elapsed time instead of hanging it.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// Measured release baseline 2026-09-12: melody2 parse + validate + first render
// (532 sections / 457 cables / 115 hw tokens) well under 1 s.
pub const PARSE_RENDER_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-09-12: melody2 graph build + full solve sub-second.
pub const GRAPH_SOLVE_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-09-12: melody2 optimizer candidate generation sub-second.
pub const OPTIMIZER_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-09-12: full gallery matrix ~0.32 s CPU.
pub const GALLERY_BUDGET: Duration = Duration::from_secs(30);

/// Run `f` on a worker thread under a wall-clock ceiling. If `f` returns within
/// `budget`, return its result. If it runs past the ceiling, panic with the phase
/// name and elapsed time so a hung phase fails the test instead of hanging the
/// suite. The leaked worker thread dies with the test process.
fn budgeted<R>(name: &str, budget: Duration, f: impl FnOnce() -> R + Send + 'static) -> R
where
    R: Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        // Disconnect on drop; send() failing is impossible here.
        let _ = tx.send(f());
    });
    let start = Instant::now();
    match rx.recv_timeout(budget) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "phase '{}' exceeded its {:?} budget after {:?}",
                name,
                budget,
                start.elapsed()
            );
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            // The sender dropped without a value: the phase panicked on the worker thread.
            panic!(
                "phase '{}' failed on the worker thread after {:?}",
                name,
                start.elapsed()
            );
        }
    }
}

#[test]
fn budgeted_returns_result_in_time() {
    assert_eq!(budgeted("smoke", Duration::from_secs(5), || 1 + 1), 2);
}
