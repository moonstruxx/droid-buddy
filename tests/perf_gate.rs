//! Profiling gate: runs core pure phases over the scale-anchor workload under
//! named wall-clock budgets so a CPU hog or endless loop fails the suite with
//! the phase name and elapsed time instead of hanging it.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use droid_tui::app::App;
use droid_tui::graph::{self, Cluster};
use droid_tui::latency::CostModel;
use droid_tui::layout;
use droid_tui::optimize::{self, OptimizeScope};
use droid_tui::patch::Patch;
use droid_tui::schema::load_schema;
use droid_tui::validation::validate_patch;

// Measured release baseline 2026-09-12: own_scale parse + validate well under 1 s.
// 2026-10-04 on own_scale (509 sections): phase_parse_render ~14 ms.
// (The terminal render phase was removed with the native-egui teardown; the
// egui paint path is covered by the gui-module shape/label tests.)
pub const PARSE_RENDER_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-09-14: own_scale graph build + full solve
// ~0.4 s (phase_graph_solve 28.35 s before the topology precompute fix:
// validate_topology's per-token influence walk rescanned every section per
// popped cable; it now walks a precomputed cable-sink index, and the wiring
// scan shares one precomputed token context; layout::solve itself is ~13 ms).
// Budget = ~25x the release worst case: the old ~28 s quadratic scan trips
// it while a slow CI runner and debug overhead stay green.
// 2026-10-04 on own_scale (509 sections): phase_graph_solve ~166 ms.
pub const GRAPH_SOLVE_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-10-03: own_scale `layout::node_world_sizes`
// (the renderer's per-frame node-size estimate, called twice per frame — scene
// build + pointer hit-testing) ~0.4 ms after the O(n + e) rewrite; 100 calls
// are ~0.04 s. It was ~155 ms per call with the previous per-node O(n·(n + e))
// recomputation, so 100 calls took ~15 s and the window froze on large patches
// (the graph settle animation forces continuous redraws). The 10 s budget also
// covers the debug-mode `build_from_patch` setup the watchdog times (the other
// graph phases use the same figure); the quadratic path took minutes there, so
// it still trips the gate while CI noise stays green.
// 2026-10-04 on own_scale (509 sections): 100 calls ~19 ms.
pub const SCENE_WIDTHS_BUDGET: Duration = Duration::from_secs(10);

// Measured release baseline 2026-09-12: own_scale optimizer candidate generation sub-second.
// 2026-10-04 on own_scale (509 sections): phase_optimizer ~12 ms.
pub const OPTIMIZER_BUDGET: Duration = Duration::from_secs(10);

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
const SCALE_ANCHOR: &str = "fixtures/own_scale.ini";

#[test]
fn phase_parse_render() {
    let elapsed = budgeted("phase_parse_render", PARSE_RENDER_BUDGET, || {
        let start = Instant::now();
        let patch = Patch::from_ini_file(Path::new(SCALE_ANCHOR)).unwrap();
        // The findings are discarded; the validation work itself is the phase.
        let _issues = validate_patch(&patch, load_schema());
        let mut app = App::new();
        app.load_patch(patch);
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
fn phase_scene_widths() {
    let elapsed = budgeted("phase_scene_widths", SCENE_WIDTHS_BUDGET, || {
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
        // 100 frames of the per-frame estimate the renderer makes twice per
        // frame (scene + hit-test): ~0.04 s after the rewrite, ~15 s before.
        let start = Instant::now();
        for _ in 0..100 {
            let sizes = layout::node_world_sizes(&g);
            std::hint::black_box(sizes.len());
        }
        start.elapsed()
    });
    eprintln!("phase_scene_widths elapsed: {elapsed:.3?}");
    assert!(
        elapsed <= SCENE_WIDTHS_BUDGET,
        "phase_scene_widths took {elapsed:?}, budget {SCENE_WIDTHS_BUDGET:?}"
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
