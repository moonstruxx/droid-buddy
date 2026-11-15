//! Shared test-only helpers (compiled only under `cfg(test)`).
//!
//! A helper that more than one module's tests need lives here instead of being
//! copied per module: the two `catalog_test_schema` copies had already drifted
//! (one declared the pulser alone, the other three circuits), so the same
//! "schema-driven" test meant two different schemas depending on the file.

use crate::graph::{Cluster, Graph, GraphOptions};
use crate::latency::CostModel;
use crate::patch::Patch;

/// A tiny catalog schema with the three circuits the register/cable tests
/// exercise: `pulser` (a boolean output the register scanner reads),
/// `buttonwriter` (a `button` output that writes a register) and `potreader` (a
/// `pot` input that reads one). Leaked on purpose — a test process makes a
/// handful of these.
pub fn catalog_schema() -> &'static crate::schema::Schema {
    let schema: crate::schema::Schema = serde_json::from_str(
        r#"{
        "firmware_version": "test",
            "jacktable_initial_size": 0,
            "available_memory": {},
            "circuits": {
                "pulser": {
                    "category": "test",
                    "title": "Pulser",
                    "description": "test circuit",
                    "ramsize": 100,
                    "inputs": [
                        {
                            "name": "input",
                            "short": "in",
                            "type": "in",
                            "description": "input",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "outputs": [
                        {
                            "name": "pulse",
                            "short": "pulse",
                            "type": "out",
                            "description": "output",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "presets": 0,
                    "manual": 0
                },
                "buttonwriter": {
                    "category": "test",
                    "title": "Button Writer",
                    "description": "writes to button registers",
                    "ramsize": 10,
                    "inputs": [],
                    "outputs": [
                        {
                            "name": "button",
                            "short": "btn",
                            "type": "out",
                            "description": "button output",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "presets": 0,
                    "manual": 0
                },
                "potreader": {
                    "category": "test",
                    "title": "Pot Reader",
                    "description": "reads from pot registers",
                    "ramsize": 10,
                    "inputs": [
                        {
                            "name": "pot",
                            "short": "pot",
                            "type": "in",
                            "description": "pot input",
                            "essential": 0,
                            "ramhint": "",
                            "autotitle": false
                        }
                    ],
                    "outputs": [],
                    "presets": 0,
                    "manual": 0
                }
            },
            "controllers": {},
            "manual_references": {}
        }"#,
    )
    .unwrap();
    Box::leak(Box::new(schema))
}

/// Four circuit nodes across two banner-group clusters, three columns wide — the
/// graph both the scene builder and the window paint tests frame. The clusters
/// matter: the sections-mode tests anchor on them and the column/force tests
/// assert their enclosing rectangles.
pub fn two_group_app() -> crate::app::App {
    use crate::graph::{Cluster, Graph, GraphEdge, GraphNode};

    let graph = Graph {
        nodes: vec![
            GraphNode {
                id: crate::patch::NodeId::circuit("clocktool", 0),
                kind: crate::graph::NodeKind::Circuit,
                circuit: "clocktool".to_string(),
                instance_index: 0,
                section_index: 0,
            },
            GraphNode {
                id: crate::patch::NodeId::circuit("osc", 0),
                kind: crate::graph::NodeKind::Circuit,
                circuit: "osc".to_string(),
                instance_index: 0,
                section_index: 0,
            },
            GraphNode {
                id: crate::patch::NodeId::circuit("vca", 0),
                kind: crate::graph::NodeKind::Circuit,
                circuit: "vca".to_string(),
                instance_index: 0,
                section_index: 1,
            },
            GraphNode {
                id: crate::patch::NodeId::circuit("filter", 0),
                kind: crate::graph::NodeKind::Circuit,
                circuit: "filter".to_string(),
                instance_index: 0,
                section_index: 1,
            },
        ],
        edges: vec![GraphEdge {
            cable: "_CLK".to_string(),
            source: crate::patch::NodeId::circuit("clocktool", 0),
            sink: crate::patch::NodeId::circuit("osc", 0),
        }],
        clusters: vec![
            Cluster {
                title: "Controls".to_string(),
                section_range: 0..2,
            },
            Cluster {
                title: "Audio".to_string(),
                section_range: 2..4,
            },
        ],
        ..Graph::default()
    };
    let mut app = crate::app::App::new();
    app.graph = Some(graph);
    app.graph_positions = vec![(0.0, 0.0), (260.0, 0.0), (520.0, 0.0), (780.0, 0.0)];
    app
}

/// A patch's graph, built the way `App::build_graph_state` builds it: the
/// patch's banner groups become clusters, with the shared default cost model and
/// graph options. Every test that needs a real fixture graph goes through here,
/// so a change to that wiring is exercised by all of them at once.
pub fn graph_for_patch(patch: &Patch) -> Graph {
    let clusters: Vec<Cluster> = patch
        .banner_groups
        .iter()
        .map(|g| Cluster {
            title: g.banner.clone().unwrap_or_default(),
            section_range: g.section_range.clone(),
        })
        .collect();
    Graph::build_from_patch(
        patch,
        &clusters,
        &CostModel::default(),
        &GraphOptions::default(),
    )
}

/// A `fixtures/` patch and its graph (see [`graph_for_patch`]). `name` is the
/// file name below `fixtures/`, e.g. `own_buttons.ini`.
pub fn fixture_patch_and_graph(name: &str) -> (Patch, Graph) {
    let patch = Patch::from_ini_file(std::path::Path::new(&format!("fixtures/{name}"))).unwrap();
    let graph = graph_for_patch(&patch);
    (patch, graph)
}

/// A `fixtures/` patch's graph, without the patch itself.
pub fn fixture_graph(name: &str) -> Graph {
    fixture_patch_and_graph(name).1
}

/// Bounding box `(width, height)` of a position set.
pub fn positions_bbox(positions: &[(f32, f32)]) -> (f32, f32) {
    let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for (x, y) in positions {
        min_x = min_x.min(*x);
        max_x = max_x.max(*x);
        min_y = min_y.min(*y);
        max_y = max_y.max(*y);
    }
    (max_x - min_x, max_y - min_y)
}
