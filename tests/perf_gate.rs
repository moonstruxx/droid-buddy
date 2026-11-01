// The budgets in this file are release timings calibrated on x86_64 Linux
// (the ubuntu-latest anchor); they are not portable. A shared Windows or macOS
// CI runner has a different CPU, which would make the gate flake, and a flaky
// gate is worse than none — so the whole integration test is Linux-only.
// CI still runs it in release on ubuntu-latest; docs/build.md documents the
// scoping. The `#![cfg]` removes the entire crate on other targets, leaving an
// empty test binary with no unused imports or items.
#![cfg(target_os = "linux")]

//! Profiling gate: runs core pure phases over the scale-anchor workload under
//! named wall-clock budgets so a CPU hog or endless loop fails the suite with
//! the phase name and elapsed time instead of hanging it.
//!
//! The module UI's paint path has no phase, deliberately (module-ui-labels task
//! 3.3, measured 2026-10-19 in release). It is the egui surface
//! (`gui::physical`), every entry point of which is `pub(crate)`, so an
//! integration test cannot reach it without widening the crate API — and a
//! phase that cannot observe the code is noise. Measured directly instead, with
//! a temporary in-crate harness since removed: a fitted 640×520 pane on the
//! scale anchor builds its spec (layout + cells + annotations) in ~0.16 ms and
//! paints warm in ~30 µs, the `budget_annotations` pass itself ~0.9 µs. The
//! largest real patch (melody2: 214 cells, 90 clipped candidates and all 90
//! dropped one at a time by the 30 % band budget) puts the annotation pass at
//! ~0.63 ms and the whole spec at ~3.2 ms. That is four orders of magnitude
//! under these budgets, and the work is bounded by construction (drops ≤
//! candidates, each a single deterministic re-placement), so only a
//! catastrophic blowup could trip a phase here.
//!
//! The per-frame label layout the fitted labels added (module-ui-label-fit task
//! 3.3) was measured the same way on 2026-11-01, in release, through a temporary
//! in-crate probe since removed: a fitted 640×520 pane, `physical::label_layout`
//! timed once per cell exactly as `gui::physical::physical_spec` calls it, then
//! a warm headless spec build and a warm headless `paint_physical` of the spec
//! just built.
//!
//! - Scale anchor (11 cells, all labelled): 1.1–1.6 µs per call, so 12–17 µs of
//!   layout inside a 0.33–0.47 ms spec build (3.5–3.9 %); warm paint 18–25 µs.
//! - Largest real patch (the corpus copy of `droid_mpfs5melody2.ini`: 214 cells,
//!   90 labelled, 82 laid out, 132 not — 124 unlabelled cells plus 8 labels no
//!   ladder step fits): 0.9–1.2 µs per call averaged over all cells and
//!   2.1–2.2 µs over the labelled ones, because the cells that fit nothing run
//!   the whole ladder plus the floor retry; so 190–265 µs inside a 9.4–9.9 ms
//!   spec build (2.0–2.8 %); warm paint 154–276 µs.
//!
//! Against the numbers above: the layout is 2–4 % of the build on both patches.
//! It replaces `label_is_clipped`, which was a few float comparisons per cell, so
//! the addition is essentially the layout scan itself — and `auto` now finds no
//! annotation candidate on either patch (module-ui-label-fit D9), so the ~0.63 ms
//! annotation pass recorded above is no longer paid at all. Both spec builds came
//! out 2–3× their recorded ~0.16 ms / ~3.2 ms while the phases in this file spanned
//! 0.7–3× their own baselines in the same window (load average ~5 on 24 cores), so
//! that delta sits in the rest of the spec build, not in these calls; the share is
//! the figure that does not drift with the machine.
//!
//! Decision: no phase, and the reason is the same one as above plus the cost.
//! ~1 µs per cell is real work, but 2–4 % of a spec build that is ~1000x under
//! every budget here cannot trip one, and the per-frame entry point is still
//! `pub(crate)`. A phase built on the `pub` `physical::label_layout` +
//! `physical::label_fit_zoom` alone would time a pure function in a tight loop —
//! a different workload from the one the frame pays — so it would guard a number
//! nobody moves; the recording above is the baseline a future change is compared
//! against instead.

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

// Measured release baseline 2026-10-15: own_scale force `layout::solve`
// (size-aware repulsion relaxation over 511 nodes + the separation
// post-condition) ~10.6 ms.
// The force path's non-overlap post-condition is a pairwise-greedy O(n^2)
// pass — ~130k body-pair tests per pass on the anchor — run once per solve for
// every pass the solver's own budget allows. Its cost is therefore node count
// squared *times* pass count, so it can only get expensive by a factor the
// solver controls (a wider pass cap, or a loop that no longer exits at zero),
// which is exactly the silent regression this phase names.
// Budget = 10 s, ~950x the measured release cost, matching the neighbouring
// phases' wide margin on purpose: their figure is the debug-mode solution cost
// the same watchdog times, and it has to stay green when task 1.5 re-tunes the
// force separation budget upward (the pass needs ~64 rounds at cohesion 0 on
// this anchor, versus the 8 the solver shipped when this phase was added), while
// still tripping on the unbounded loop that is the real failure mode.
// 2026-10-15 on own_scale (511 nodes): phase_force_solve_and_separation ~10.6 ms.
pub const FORCE_SOLVE_BUDGET: Duration = Duration::from_secs(10);

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
        let positions = layout::solve(&g, &[], layout::SolveKnobs::default());
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

/// Guards the **force** arrangement's end-to-end solve cost at scale: the
/// relaxation over 511 anchor nodes *plus* the non-overlap post-condition
/// (`layout::separate_overlaps`, a bounded pairwise-greedy pass over every
/// `(i, j)` body pair — O(n^2) per pass).
///
/// The pass exists so "no two drawn bodies intersect" holds by construction
/// rather than by tuning, and while it was invisibly cheap the cost was fine;
/// but its work is node-count squared times pass count, so the two ways it
/// regresses are silent (a wider pass cap, or an unbounded loop) and both show
/// up only as a frozen frame on a large patch. This phase runs whatever the
/// solver does at its default knobs and turns that freeze into a named failure.
///
/// Deliberately no hardcoded pass count or knob value: the phase must measure
/// the solver's shipped defaults, so re-tuning them cannot make the gate lie.
#[test]
fn phase_force_solve_and_separation() {
    let elapsed = budgeted(
        "phase_force_solve_and_separation",
        FORCE_SOLVE_BUDGET,
        || {
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
            // The anchor build itself is gated by phase_graph_solve; the timer
            // wraps only what this phase is the guard for — the solve and its
            // separation post-condition, at the solver's default knobs.
            let start = Instant::now();
            let positions = layout::solve(&g, &[], layout::SolveKnobs::default());
            // The solver must return one position per node.
            assert_eq!(positions.len(), g.nodes.len());
            start.elapsed()
        },
    );
    eprintln!("phase_force_solve_and_separation elapsed: {elapsed:.3?}");
    assert!(
        elapsed <= FORCE_SOLVE_BUDGET,
        "phase_force_solve_and_separation took {elapsed:?}, budget {FORCE_SOLVE_BUDGET:?}"
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
