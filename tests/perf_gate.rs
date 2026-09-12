//! Profiling gate: runs core pure phases over the scale-anchor workload under
//! named wall-clock budgets so a CPU hog or endless loop fails the suite with
//! the phase name and elapsed time instead of hanging it.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use droid_tui::app::App;
use droid_tui::gallery;
use droid_tui::graph::{self, Cluster};
use droid_tui::latency::CostModel;
use droid_tui::layout;
use droid_tui::optimize::{self, OptimizeScope};
use droid_tui::patch::Patch;
use droid_tui::schema::load_schema;
use droid_tui::validation::validate_patch;

// Measured release baseline 2026-09-12: melody2 parse + validate + first render
// (532 sections / 457 cables / 115 hw tokens) well under 1 s.
pub const PARSE_RENDER_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-09-12: melody2 graph build + full solve
// ~19 s (build 13 ms; the solve runs the full 60-iteration budget because the
// energy threshold is not reached on this machine, so the design's sub-second
// baseline, which reflected an early freeze, does not reproduce here). Debug
// measures 84-96 s. Budget = ~10x the release worst case: an order-of-magnitude
// regression (~190 s) still trips it (design D2) while a slow CI runner and
// debug overhead stay green.
pub const GRAPH_SOLVE_BUDGET: Duration = Duration::from_secs(200);

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

// The scale anchor (design D4): the largest real patch in the corpus, so the
// measured worst case for every gated phase.
const SCALE_ANCHOR: &str = "fixtures/droid_mpfs5melody2.ini";

#[test]
fn phase_parse_render() {
    let elapsed = budgeted("phase_parse_render", PARSE_RENDER_BUDGET, || {
        let start = Instant::now();
        let patch = Patch::from_ini_file(Path::new(SCALE_ANCHOR)).unwrap();
        // The findings are discarded; the validation work itself is the phase.
        let _issues = validate_patch(&patch, load_schema());
        let mut app = App::new();
        app.load_patch(patch);
        gallery::buffer_for(&mut app, 120, 40);
        start.elapsed()
    });
    eprintln!("phase_parse_render elapsed: {elapsed:.3?}");
    assert!(
        elapsed <= PARSE_RENDER_BUDGET,
        "phase_parse_render took {elapsed:?}, budget {PARSE_RENDER_BUDGET:?}"
    );
}

#[test]
fn phase_graph_solve() {
    let elapsed = budgeted("phase_graph_solve", GRAPH_SOLVE_BUDGET, || {
        let start = Instant::now();
        let patch = Patch::from_ini_file(Path::new(SCALE_ANCHOR)).unwrap();
        // Mirrors app::clusters_from_patch, which is private to the crate.
        let clusters: Vec<Cluster> = patch
            .banner_groups
            .iter()
            .map(|group| Cluster {
                title: group.banner.as_deref().unwrap_or("(unnamed)").to_string(),
                section_range: group.section_range.clone(),
            })
            .collect();
        let cost = CostModel::default();
        let options = graph::GraphOptions::default();
        let g = graph::Graph::build_from_patch(&patch, &clusters, &cost, &options);
        let positions = layout::solve(&g, &[], layout::DEFAULT_TENSION);
        // The solver must return one position per node.
        assert_eq!(positions.len(), g.nodes.len());
        start.elapsed()
    });
    eprintln!("phase_graph_solve elapsed: {elapsed:.3?}");
    assert!(
        elapsed <= GRAPH_SOLVE_BUDGET,
        "phase_graph_solve took {elapsed:?}, budget {GRAPH_SOLVE_BUDGET:?}"
    );
}

#[test]
fn phase_optimizer() {
    let elapsed = budgeted("phase_optimizer", OPTIMIZER_BUDGET, || {
        let start = Instant::now();
        let patch = Patch::from_ini_file(Path::new(SCALE_ANCHOR)).unwrap();
        let cost = CostModel::default();
        // MinMax runs all three strategies, the heaviest representative workload.
        let candidates = optimize::generate_candidates(&patch, &cost, OptimizeScope::MinMax);
        assert!(!candidates.is_empty(), "optimizer produced no candidates");
        start.elapsed()
    });
    eprintln!("phase_optimizer elapsed: {elapsed:.3?}");
    assert!(
        elapsed <= OPTIMIZER_BUDGET,
        "phase_optimizer took {elapsed:?}, budget {OPTIMIZER_BUDGET:?}"
    );
}

#[test]
fn phase_gallery() {
    let elapsed = budgeted("phase_gallery", GALLERY_BUDGET, || {
        let start = Instant::now();
        // Reuses the in-suite harness: every scenario x theme, writing the
        // ephemeral, gitignored evidence/gallery/ outputs.
        let index = gallery::generate_gallery().unwrap();
        assert!(
            index.is_file(),
            "gallery index missing: {}",
            index.display()
        );
        start.elapsed()
    });
    eprintln!("phase_gallery elapsed: {elapsed:.3?}");
    assert!(
        elapsed <= GALLERY_BUDGET,
        "phase_gallery took {elapsed:?}, budget {GALLERY_BUDGET:?}"
    );
}
