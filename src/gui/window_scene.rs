//! Window scene construction: the live `App` graph state resolved into the
//! identity-carrying [`SceneSpec`] that the egui painter consumes (design D9 /
//! gpu-graph-window).
//!
//! The builder re-derives from the shared [`GraphCamera`] each frame (seeded
//! once with fit-the-graph), so graph rebuilds, pan/zoom, and theme changes
//! never leave the window stale. Colors resolve through the classification
//! chain the terminal graph used (error > dim > diff > latency ramp > kind,
//! with the register token keyed above diff), hitting the per-kind, register,
//! and latency theme tokens in one place.

use std::collections::{HashMap, HashSet};

use crate::app::App;
use crate::graph::{Graph, GraphEdge, GraphNode, NodeId, NodeKind};
use crate::graph_render::{
    cluster_rect_from_members, CableKind as SpecCableKind, EdgeDiffState, EdgeLatency, EdgeSpec,
    GraphCamera, NodeSpec, SceneSpec, WorldBounds,
};
use crate::patch::Patch;

/// Camera-fit zoom floor, pixels per world unit: the tightest typical node
/// spacing (`layout::SPRING_REST` ≈ 80 world units) must render a node at its
/// legible width in points.
const GRAPH_MIN_NODE_PX: f32 = crate::graph_render::GRAPH_WINDOW_NODE_W / 80.0;

/// Window cluster margin (design D5): the egui painter draws the container as
/// a plain border, so the margin only keeps the frame clear of the member node
/// edges, mirroring the box path's `GRAPH_CLUSTER_PADDING` cells.
const WINDOW_CLUSTER_PADDING: f32 = 12.0;

/// Camera fit for the graph window (design D5): frames the whole node set with
/// one node margin, against the window's initial 1280×800 logical size.
/// Seeded once; later frames reuse the pan/zoomed camera
/// (`handle_graph_window_frame` mutates `App::graph_camera`).
fn graph_window_fit_camera(positions: &[(f32, f32)]) -> GraphCamera {
    let avail_w = (1280.0 - crate::graph_render::GRAPH_WINDOW_NODE_W).max(1.0);
    let avail_h = (800.0 - crate::graph_render::GRAPH_WINDOW_NODE_H).max(1.0);
    GraphCamera::fit_to_world(
        WorldBounds::from_positions(positions),
        (avail_w, avail_h),
        GRAPH_MIN_NODE_PX,
    )
}

/// Classification + hover state for the window path, grouped so the pixel
/// helpers stay under clippy's 7-argument limit.
#[derive(Clone, Copy)]
struct GraphSceneOpts<'a> {
    disabled: &'a HashSet<NodeId>,
    diff_report: Option<&'a crate::diff::DiffReport>,
    diff_showing: bool,
    latency_coloring: bool,
    hovered: Option<usize>,
    selected: Option<&'a NodeId>,
    patch: Option<&'a Patch>,
    circuit_store: &'a HashMap<NodeId, String>,
    /// Dependency-subset node indices (empty = unfiltered, change D task 3.1).
    dep_nodes: &'a [usize],
    /// Dependency-subset edge indices (empty = unfiltered, change D task 3.1).
    dep_edges: &'a [usize],
}

/// Color for one cable on the window path — the pixel analogue of the cable
/// precedence chain (error red over disabled dim over diff over latency ramp
/// over kind, resolved by [`cable_color_with_diff`]).
fn graph_edge_pixel_color(
    graph: &Graph,
    edge: &GraphEdge,
    src_node: &GraphNode,
    sink_node: &GraphNode,
    edge_index: usize,
    opts: GraphSceneOpts<'_>,
) -> crate::theme::Color {
    let theme = crate::theme::active();
    let incident_disabled =
        circuit_disabled(opts.disabled, &src_node.circuit, src_node.instance_index)
            || circuit_disabled(opts.disabled, &sink_node.circuit, sink_node.instance_index);
    let has_error = graph
        .validation
        .iter()
        .any(|issue| issue.cable == edge.cable);
    if incident_disabled {
        // Error red outranks the dim (error > dim > influence > kind).
        if has_error {
            return theme.graph_edge_error;
        }
        return theme.graph_edge_dim;
    }
    if has_error {
        return theme.graph_edge_error;
    }
    if opts.diff_showing {
        if let Some(report) = opts.diff_report {
            if report.added_cables.contains(&edge.cable) {
                return theme.graph_edge_diff_added;
            }
            if report.removed_cables.contains(&edge.cable) {
                return theme.graph_edge_diff_removed;
            }
            if report.changed_cables.iter().any(|c| c.cable == edge.cable) {
                return theme.graph_edge_diff_added;
            }
        }
    }
    // Latency ramp replaces the kind color when coloring is on (error > diff
    // > ramp > kind); a back-edge always lands on the hottest stop.
    if opts.latency_coloring {
        if let Some(data) = graph.latency.as_ref() {
            if let Some(entry) = data.edges.get(edge_index) {
                let ramp = theme.graph_edge_latency_ramp();
                let stop = if entry.is_back_edge {
                    ramp.len() - 1
                } else {
                    latency_ramp_index(
                        entry.latency,
                        data.edges.len(),
                        data.summary.avg,
                        ramp.len(),
                    )
                };
                return ramp[stop];
            }
        }
    }
    cable_color_with_diff(graph, &edge.cable, opts.diff_report, opts.diff_showing)
}

/// Resolve one node's pixel border/title colors and display label (the chain:
/// disabled/not-selected dim, then selected/hover highlight, then per-kind
/// frames), plus the `circuit_display_label` title and the diff-`*` suffix.
/// Returns `(border_color, title_color, label)`.
fn graph_node_pixel_style(
    graph: &Graph,
    node: &GraphNode,
    node_index: usize,
    opts: GraphSceneOpts<'_>,
) -> (crate::theme::Color, crate::theme::Color, String) {
    let theme = crate::theme::active();
    let is_disabled = circuit_disabled(opts.disabled, &node.circuit, node.instance_index);
    let is_not_selected = graph.not_selected.contains(&node.section_index);
    let (border, title_color) = if is_disabled || is_not_selected {
        (theme.graph_node_dim, theme.graph_node_dim)
    } else if opts.selected == Some(&node.id) || opts.hovered == Some(node_index) {
        (theme.graph_node_highlight, theme.graph_node_highlight)
    } else {
        match node.kind {
            NodeKind::Circuit => (theme.graph_node_border, theme.graph_node_title),
            NodeKind::Controller => (theme.graph_node_controller, theme.graph_node_controller),
            NodeKind::InputJack => (theme.graph_node_jack_input, theme.graph_node_jack_input),
            NodeKind::OutputJack => (theme.graph_node_jack_output, theme.graph_node_jack_output),
        }
    };
    let mut label = graph_node_display_title(node, opts.patch, Some(opts.circuit_store));
    if opts.diff_showing
        && opts.diff_report.is_some_and(|r| {
            r.changed_nodes.iter().any(|n| n.id == node.id)
                || r.added_nodes.contains(&node.id)
                || r.removed_nodes.contains(&node.id)
        })
    {
        label.push('*');
    }
    (border, title_color, label)
}

/// Build the window's `SceneSpec` for one frame (design D9): nodes with
/// identity + port flags, edges with kind/diff/latency state, and cluster
/// containers. Positions map through the shared [`GraphCamera`], seeded on the
/// first frame with [`graph_window_fit_camera`] and reused after (pan/zoom).
/// `None` while no graph is loaded or the positions/node count disagree, so
/// the window paints an empty canvas.
pub fn build_window_scene_spec(app: &mut App) -> Option<SceneSpec> {
    let lens_match = app
        .graph
        .as_ref()
        .map_or(false, |g| app.graph_positions.len() == g.nodes.len());
    let _ = lens_match;
    let graph = app.graph.as_ref()?;
    if app.graph_positions.len() != graph.nodes.len() {
        return None;
    }

    if app.graph_camera.is_none() {
        // Fit the camera over the dependency subset's positions while the
        // filter is active, so the cut reads as its own scene (change D).
        let fit_src: Vec<(f32, f32)> = if app.dependency_nodes.is_empty() {
            app.graph_positions.clone()
        } else {
            app.dependency_nodes
                .iter()
                .map(|&i| app.graph_positions[i])
                .collect()
        };
        app.graph_camera = Some(graph_window_fit_camera(&fit_src));
    }
    let cam = app.graph_camera?;
    let theme = crate::theme::active();

    // Copyable state + owned report captured before the field destructure.
    let circuit_store = app.current_circuit_store();
    let patch_for_title = app.patch.clone();
    let diff_report_owned = app.filtered_report();
    let selected_circuit = app.selected_circuit.clone();
    let dependency_nodes = app.dependency_nodes.clone();
    let dependency_edges = app.dependency_edges.clone();
    let diff_showing = app.diff_showing;
    let latency_coloring = app.latency_coloring;
    let hovered = app.hovered_graph_node;
    let filtered = !dependency_nodes.is_empty();

    let App {
        graph,
        graph_positions,
        disabled_circuits,
        ..
    } = app;
    let graph = graph.as_ref()?;

    let node_w = crate::graph_render::GRAPH_WINDOW_NODE_W;
    let node_h = crate::graph_render::GRAPH_WINDOW_NODE_H;
    let opts = GraphSceneOpts {
        disabled: disabled_circuits,
        diff_report: diff_report_owned.as_ref(),
        diff_showing,
        latency_coloring,
        hovered,
        selected: selected_circuit.as_ref(),
        patch: patch_for_title.as_ref(),
        circuit_store: &circuit_store,
        dep_nodes: &dependency_nodes,
        dep_edges: &dependency_edges,
    };

    // Nodes: `node_topleft` stays index-aligned (skipped nodes get (0,0)) so
    // the edge pass resolves source/sink positions; `nodes` only carries the
    // visible subset, matching the kitty path's dependency filter.
    let mut node_topleft: Vec<(f32, f32)> = Vec::with_capacity(graph.nodes.len());
    let mut nodes: Vec<NodeSpec> = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let in_subset = !filtered || dependency_nodes.contains(&i);
        let (px, py) = if in_subset {
            let (wx, wy) = graph_positions[i];
            cam.world_to_pixel(wx, wy)
        } else {
            (0.0, 0.0)
        };
        node_topleft.push((px, py));
        if !in_subset {
            continue;
        }
        let (border, label_color, label) = graph_node_pixel_style(graph, node, i, opts);
        let input_port = graph.edges.iter().any(|e| e.sink == node.id);
        let output_port = graph.edges.iter().any(|e| e.source == node.id);
        nodes.push(NodeSpec {
            x: px,
            y: py,
            w: node_w,
            h: node_h,
            radius: 8.0,
            fill: theme.rgb(theme.graph_node_fill),
            border: theme.rgb(border),
            border_width: 3.0,
            label,
            label_color: theme.rgb(label_color),
            circuit: node.circuit.clone(),
            instance_index: node.instance_index,
            input_port,
            output_port,
        });
    }

    // Edges: quadratic from the source's right-center to the sink's
    // left-center, mirroring the kitty path's geometry, plus the semantic
    // state the window painter needs for legends/tooltips.
    let mut edges: Vec<EdgeSpec> = Vec::with_capacity(graph.edges.len());
    for (edge_index, edge) in graph.edges.iter().enumerate() {
        if filtered && !dependency_edges.is_empty() && !dependency_edges.contains(&edge_index) {
            continue;
        }
        let Some(src) = graph.nodes.iter().position(|n| n.id == edge.source) else {
            continue;
        };
        let Some(sink) = graph.nodes.iter().position(|n| n.id == edge.sink) else {
            continue;
        };
        let (sx, sy) = node_topleft[src];
        let (tx, ty) = node_topleft[sink];
        let start = (sx + node_w, sy + node_h / 2.0);
        let end = (tx, ty + node_h / 2.0);
        if start == end {
            continue; // coincident nodes: zero-length edge
        }
        let ctrl = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
        let color = graph_edge_pixel_color(
            graph,
            edge,
            &graph.nodes[src],
            &graph.nodes[sink],
            edge_index,
            opts,
        );
        let has_error = graph
            .validation
            .iter()
            .any(|issue| issue.cable == edge.cable);
        let incident_disabled = circuit_disabled(
            opts.disabled,
            &graph.nodes[src].circuit,
            graph.nodes[src].instance_index,
        ) || circuit_disabled(
            opts.disabled,
            &graph.nodes[sink].circuit,
            graph.nodes[sink].instance_index,
        );
        let kind = match cable_kind(graph, &edge.cable) {
            CableKind::Control => SpecCableKind::Control,
            CableKind::Audio => SpecCableKind::Audio,
            CableKind::Midi => SpecCableKind::Midi,
            CableKind::Unknown => SpecCableKind::Unknown,
        };
        let diff = if diff_showing {
            opts.diff_report.and_then(|report| {
                if report.added_cables.contains(&edge.cable) {
                    Some(EdgeDiffState::Added)
                } else if report.removed_cables.contains(&edge.cable) {
                    Some(EdgeDiffState::Removed)
                } else if report.changed_cables.iter().any(|c| c.cable == edge.cable) {
                    Some(EdgeDiffState::Changed)
                } else {
                    None
                }
            })
        } else {
            None
        };
        let latency = if latency_coloring {
            graph.latency.as_ref().and_then(|data| {
                data.edges.get(edge_index).map(|entry| {
                    let ramp = theme.graph_edge_latency_ramp();
                    let ramp_stop = if entry.is_back_edge {
                        ramp.len() - 1
                    } else {
                        latency_ramp_index(
                            entry.latency,
                            data.edges.len(),
                            data.summary.avg,
                            ramp.len(),
                        )
                    };
                    EdgeLatency {
                        ramp_stop,
                        back_edge: entry.is_back_edge,
                    }
                })
            })
        } else {
            None
        };
        edges.push(EdgeSpec {
            start,
            end,
            ctrl,
            color: theme.rgb(color),
            width: 3.0,
            kind,
            error: has_error,
            dim: incident_disabled,
            diff,
            latency,
        });
    }

    // Clusters: banner-group containers, skipped while the dependency filter
    // is active (mirrors the box path). Member indices are original node
    // indices; unfiltered, `nodes` carries every node in order, so they index
    // `nodes`.
    let mut clusters = Vec::new();
    if !filtered {
        for cluster in &graph.clusters {
            let member_indices: Vec<usize> = graph
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| cluster.section_range.contains(&n.section_index))
                .map(|(i, _)| i)
                .collect();
            let (border, title_color) = {
                let mut b = theme.graph_cluster_border;
                let mut t = theme.graph_cluster_title;
                if diff_showing {
                    if let Some(report) = opts.diff_report {
                        let all_added = !member_indices.is_empty()
                            && member_indices
                                .iter()
                                .all(|&i| report.added_nodes.contains(&graph.nodes[i].id));
                        let all_removed = !member_indices.is_empty()
                            && member_indices
                                .iter()
                                .all(|&i| report.removed_nodes.contains(&graph.nodes[i].id));
                        if all_added {
                            b = theme.graph_edge_diff_added;
                            t = theme.graph_edge_diff_added;
                        } else if all_removed {
                            b = theme.graph_edge_diff_removed;
                            t = theme.graph_edge_diff_removed;
                        }
                    }
                }
                (b, t)
            };
            let Some((x, y, w, h)) =
                cluster_rect_from_members(&member_indices, &nodes, WINDOW_CLUSTER_PADDING)
            else {
                continue;
            };
            clusters.push(crate::graph_render::ClusterSpec {
                x,
                y,
                w,
                h,
                title: cluster.title.clone(),
                border: theme.rgb(border),
                title_color: theme.rgb(title_color),
                member_indices,
            });
        }
    }

    Some(SceneSpec {
        background: theme.rgb(theme.graph_canvas_bg),
        nodes,
        edges,
        clusters,
    })
}

/// Title text for a node's frame: the circuit name, with the zero-based
/// instance index appended only when the name is repeated (instance > 0).
fn graph_node_title(node: &GraphNode) -> String {
    if node.instance_index == 0 {
        node.circuit.clone()
    } else {
        format!("{} {}", node.circuit, node.instance_index)
    }
}

/// Circuit-label override for a node's title: `Patch::circuit_display_label`
/// when a patch and store are available, otherwise the derived title.
fn graph_node_display_title(
    node: &GraphNode,
    patch: Option<&Patch>,
    circuit_store: Option<&HashMap<NodeId, String>>,
) -> String {
    if let (Some(patch), Some(store)) = (patch, circuit_store) {
        patch.circuit_display_label(&node.id, store)
    } else {
        graph_node_title(node)
    }
}

/// Whether a circuit instance has processing disabled: `App.disabled_circuits`
/// is keyed by `(circuit name, instance index)`, the same identity a
/// `GraphNode` carries (`circuit` + `instance_index`).
fn circuit_disabled(disabled: &HashSet<NodeId>, circuit: &str, instance_index: usize) -> bool {
    disabled.contains(&NodeId::circuit(circuit, instance_index))
}

/// The producing circuit of a cable: the source end of the first edge carrying
/// it, resolved to a node's circuit name.
fn cable_source_circuit<'a>(graph: &'a Graph, cable: &str) -> Option<&'a str> {
    let source = graph
        .edges
        .iter()
        .find(|e| e.cable == cable)?
        .source
        .clone();
    graph
        .nodes
        .iter()
        .find(|n| n.id == source)
        .map(|n| n.circuit.as_str())
}

/// Cable kind inferred from its producing circuit; `Unknown` when no edge
/// produces it.
fn cable_kind(graph: &Graph, cable: &str) -> CableKind {
    match cable_source_circuit(graph, cable) {
        Some(circuit) => CableKind::from_circuit(circuit),
        None => CableKind::Unknown,
    }
}

fn cable_color_with_diff(
    graph: &Graph,
    cable: &str,
    diff_report: Option<&crate::diff::DiffReport>,
    diff_showing: bool,
) -> crate::theme::Color {
    let theme = crate::theme::active();
    if graph.validation.iter().any(|issue| issue.cable == cable) {
        return theme.graph_edge_error;
    }
    // Register edges (task 4.1): `_REG:`-prefixed cables use the register
    // token instead of the cable-kind-based color. Precedence: error >
    // register > diff > latency ramp > cable kind.
    if cable.starts_with("_REG:") {
        return theme.graph_edge_register;
    }
    if diff_showing {
        if let Some(report) = diff_report {
            if report.added_cables.contains(&cable.to_string()) {
                return theme.graph_edge_diff_added;
            }
            if report.removed_cables.contains(&cable.to_string()) {
                return theme.graph_edge_diff_removed;
            }
            if report.changed_cables.iter().any(|c| c.cable == cable) {
                return theme.graph_edge_diff_added;
            }
        }
    }
    match cable_kind(graph, cable) {
        CableKind::Control => theme.graph_edge_control,
        CableKind::Audio => theme.graph_edge_audio,
        CableKind::Midi => theme.graph_edge_midi,
        CableKind::Unknown => theme.graph_edge_unknown,
    }
}

/// Ramp stop for a cable's latency (design D2):
/// `round(L / (N×AVG) × (stops−1))` clamped to `stops−1`; degenerate inputs
/// (no edges or zero mean) collapse to the cold end.
fn latency_ramp_index(latency: f32, edge_count: usize, avg: f32, stops: usize) -> usize {
    if edge_count == 0 || avg <= 0.0 || stops == 0 {
        return 0;
    }
    let normalized = latency / (edge_count as f32 * avg) * (stops as f32 - 1.0);
    (normalized.round() as usize).min(stops.saturating_sub(1))
}

/// Inferred cable type for edge coloring (design D8); a visual aid only —
/// topology and validation never depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CableKind {
    Control,
    Audio,
    Midi,
    Unknown,
}

impl CableKind {
    /// Classify a producing circuit's output: clock/gate/trigger/pulsar/div
    /// emit control signals; midi/note/seq/pitch emit musical/midi signals;
    /// anything else is treated as audio/CV. A declared `cable_kind` overrides
    /// the substring guess (schema-precedence contract).
    fn from_circuit(circuit: &str) -> CableKind {
        let name = circuit.to_ascii_lowercase();
        let declared = crate::schema::load_schema()
            .circuits
            .get(&name)
            .and_then(|c| c.cable_kind.as_deref());
        if let Some(kind) = declared {
            match kind.to_ascii_lowercase().as_str() {
                "control" => return CableKind::Control,
                "midi" => return CableKind::Midi,
                "audio" => return CableKind::Audio,
                "unknown" => return CableKind::Unknown,
                _ => {}
            }
        }
        if ["clock", "gate", "trigger", "pulsar", "div"]
            .iter()
            .any(|k| name.contains(k))
        {
            CableKind::Control
        } else if ["midi", "note", "seq", "pitch"]
            .iter()
            .any(|k| name.contains(k))
        {
            CableKind::Midi
        } else {
            CableKind::Audio
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_app() -> App {
        let content = "\
# ---- Pulsar ----
[clocktool]
    output = _CLK
[copy]
    input = _CLK
# ---- Steady ----
[osc]
    input = _CLK
[p2b8]
";
        let mut app = App::new();
        let patch = Patch::from_ini_str(content, String::from("g")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        app
    }

    /// The window scene spec (task 1.1/2.1) carries node identity + ports for
    /// the painter and re-derives fresh from App state: panning the shared
    /// camera moves the pixel positions on the next frame.
    #[test]
    fn build_window_scene_spec_carries_identity_and_tracks_camera() {
        let mut app = graph_app();
        let spec = build_window_scene_spec(&mut app).expect("scene builds for a loaded graph");
        let graph = app.graph.as_ref().unwrap();
        assert_eq!(spec.nodes.len(), graph.nodes.len());
        assert_eq!(spec.edges.len(), graph.edges.len());
        assert_eq!(spec.clusters.len(), graph.clusters.len());
        for (spec_node, graph_node) in spec.nodes.iter().zip(graph.nodes.iter()) {
            assert_eq!(spec_node.circuit, graph_node.circuit);
            assert!(spec_node.x.is_finite() && spec_node.y.is_finite());
            assert!(spec_node.w > 0.0 && spec_node.h > 0.0);
            assert_eq!(
                spec_node.input_port,
                graph.edges.iter().any(|e| e.sink == graph_node.id)
            );
        }
        let before = spec.nodes[0].x;
        app.graph_camera.as_mut().unwrap().pan_by(120.0, 0.0);
        let rederived = build_window_scene_spec(&mut app).expect("scene re-derives after pan");
        assert!(
            (rederived.nodes[0].x - (before - 120.0)).abs() < 0.01,
            "pan must shift the node's pixel x by -120 ({before} -> {})",
            rederived.nodes[0].x
        );
    }

    #[test]
    fn scene_is_none_without_patch_or_positions() {
        let mut app = App::new();
        assert!(build_window_scene_spec(&mut app).is_none());

        // Open the graph without a patch: the model exists but the scene has
        // no world state without positions.
        app.open_graph();
        assert!(
            build_window_scene_spec(&mut app).is_none(),
            "no patch -> no graph model"
        );

        // Consistent state: patch + graph + positions maps through the camera.
        let mut app = graph_app();
        // A camera without a fit must not panic on degenerate graphs.
        app.graph_positions.clear();
        app.graph_camera = None;
        assert!(build_window_scene_spec(&mut app).is_none());
    }

    #[test]
    fn cable_kind_classifies_by_producing_circuit() {
        let mut app = graph_app();
        let graph = app.graph.as_ref().unwrap();
        let clk_cable = &graph.edges[0].cable;
        assert_eq!(cable_kind(graph, clk_cable), CableKind::Control);
        assert_eq!(cable_kind(graph, "_AUD"), CableKind::Unknown);
    }
}
