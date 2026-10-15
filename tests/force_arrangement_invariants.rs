//! Cross-layer, public-API cover for the arrangements' non-overlap contract.
//!
//! Task 3.1 pins the same invariant from *inside* the crate (`src/layout.rs`
//! unit tests with the private `graph_from_fixture` helper and a private
//! penetration predicate). This file is the complementary half: it drives the
//! path the application actually uses — `Patch::from_ini_file` →
//! `Graph::build_from_patch` → the exported solver entry points — and measures
//! overlap with the exported `node_world_sizes`, the renderer's own body sizes.
//! So a regression in the public surface (a load path that builds a different
//! graph, a solver entry point whose signature or pins changed, a size estimate
//! that outgrows its reserved slot) fails here even when the crate-internal
//! helper still passes.
//!
//! Two properties, both at real fixture scale:
//!
//! 1. **Force mode** — `layout::solve` returns zero intersecting axis-aligned
//!    body pairs on the measured fixture set (design D3). `tests/perf_gate.rs`
//!    runs this call on the scale anchor for time only; no fixture-size overlap
//!    assertion exists there.
//! 2. **Column mode** — `layout::solve_columns_pinned` (the app's
//!    `solve_graph_positions` call, `[layout]` ordering included) returns zero
//!    intersecting pairs for **every** `LayoutOrdering`. The ordering is a user
//!    setting, and the crate-internal column overlap tests only ever run
//!    `Strict` on synthetic graphs, so a non-default ordering that lets two
//!    bodies share a slot was unguarded.
//!
//! Replaces `tests/force_overlap_probe.rs`, the temporary recon-lane
//! measurement probe (cancelled lane). The probe's overlap counts at HEAD
//! (own_scale 224, graph_dependency_walk 33, …) and its cohesion/cutoff
//! diagnostics are historical measurements recorded in the change's `design.md`
//! and commit messages; they are not kept here as ratchet constants.

use std::path::Path;

use droid_tui::config::LayoutOrdering;
use droid_tui::graph::{Cluster, Graph, GraphOptions};
use droid_tui::latency::CostModel;
use droid_tui::layout::{self, SolveKnobs};
use droid_tui::patch::Patch;

/// The measured fixture set (task 3.1). `droid_mpfs5melody2` is not part of
/// `fixtures/`, so it has no presence guard here: the whole set must resolve.
const FIXTURES: &[&str] = &[
    "fixtures/own_scale.ini",
    "fixtures/cable_banner_combos.ini",
    "fixtures/graph_latency_chain.ini",
    "fixtures/own_two_ctrl.ini",
    "fixtures/graph_dependency_walk.ini",
];

/// Build the signal-flow graph exactly as the app does: `banner_groups` become
/// clusters (mirrors `app::clusters_from_patch`), default cost model and graph
/// options. Parsing is asserted, not unwrapped, so a corpus change reports the
/// fixture that broke instead of a bare panic.
fn graph_from_fixture(name: &str) -> Graph {
    let patch = Patch::from_ini_file(Path::new(name))
        .unwrap_or_else(|err| panic!("{name}: fixture must parse: {err}"));
    let clusters: Vec<Cluster> = patch
        .banner_groups
        .iter()
        .map(|group| Cluster {
            title: group.banner.as_deref().unwrap_or("(unnamed)").to_string(),
            section_range: group.section_range.clone(),
        })
        .collect();
    Graph::build_from_patch(
        &patch,
        &clusters,
        &CostModel::default(),
        &GraphOptions::default(),
    )
}

/// Assert no two node bodies intersect: over every pair, at most one axis may
/// separate the bodies. Bounding boxes are axis-aligned and centred on the
/// position, so the same predicate the solver's separation pass uses applies
/// (`|dx| < (wa+wb)/2` **and** `|dy| < (ha+hb)/2` ⇒ the drawn frames overlap).
fn assert_no_overlapping_bodies(sizes: &[(f32, f32)], positions: &[(f32, f32)], what: &str) {
    assert_eq!(
        sizes.len(),
        positions.len(),
        "{what}: one size and one position per node"
    );
    for a in 0..positions.len() {
        for b in a + 1..positions.len() {
            let dx = (positions[a].0 - positions[b].0).abs();
            let dy = (positions[a].1 - positions[b].1).abs();
            let need_x = 0.5 * (sizes[a].0 + sizes[b].0);
            let need_y = 0.5 * (sizes[a].1 + sizes[b].1);
            assert!(
                dx >= need_x || dy >= need_y,
                "{what}: nodes {a} and {b} overlap — |dx|={dx} < {need_x} and \
                 |dy|={dy} < {need_y}; positions {a}={:?} {b}={:?}",
                positions[a],
                positions[b]
            );
        }
    }
}

#[test]
fn force_solve_has_no_overlapping_bodies_on_the_fixture_set() {
    let knobs = SolveKnobs::default();
    for name in FIXTURES {
        let graph = graph_from_fixture(name);
        assert!(!graph.nodes.is_empty(), "{name}: fixture graph is empty");
        let sizes = layout::node_world_sizes(&graph);
        let positions = layout::solve(&graph, &[], knobs);
        assert_eq!(
            positions.len(),
            graph.nodes.len(),
            "{name}: one position per node"
        );
        assert_no_overlapping_bodies(&sizes, &positions, &format!("{name}: force solve"));
    }
}

#[test]
fn column_arrangement_has_no_overlapping_bodies_on_the_fixture_set() {
    for name in FIXTURES {
        let graph = graph_from_fixture(name);
        let sizes = layout::node_world_sizes(&graph);
        let widths = layout::estimated_widths(&graph);
        for ordering in [
            LayoutOrdering::Strict,
            LayoutOrdering::Barycenter,
            LayoutOrdering::PatchOrder,
        ] {
            // The app's column path: width-aware grid placement, no pins.
            let positions = layout::solve_columns_pinned(&graph, &widths, ordering, &[]);
            assert_eq!(
                positions.len(),
                graph.nodes.len(),
                "{name}: one position per node"
            );
            assert_no_overlapping_bodies(
                &sizes,
                &positions,
                &format!("{name}: column mode ({ordering:?})"),
            );
        }
    }
}
