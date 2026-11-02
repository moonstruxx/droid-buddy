//! Headless analysis report (`analyze`) — the curated payload the CLI prints.
//!
//! The report is a deliberate public contract, not a serialized model dump
//! (design D2): it names the facts an agent asks about and nothing else, and
//! `report_version` is the compatibility lever. Consumers are the `cli`
//! module's JSON serializer (`analyze --json`) and the end-to-end contract in
//! `tests/cli.rs`; the human text renderer reads the same structs.
//!
//! Pure module: it reads a parsed [`Patch`], the [`Schema`], an already-built
//! [`Graph`] and the caller's byte count/plugin list. It performs no I/O and
//! builds nothing beyond [`rack::resolve`] and [`validation::validate_patch`].

use serde::Serialize;

use crate::graph::{Graph, GraphEdge, NodeId, TopologySeverity};
use crate::latency::{EdgeLatency, LatencySummary};
use crate::optimize::{CandidateOrdering, OptimizeScope};
use crate::patch::Patch;
use crate::rack::{self, MasterSource};
use crate::schema::Schema;
use crate::validation::{self, Severity};

/// Report schema version. Bumped only when a reported fact changes shape;
/// consumed by the `cli` serializer and asserted by `tests/cli.rs`.
pub const REPORT_VERSION: u32 = 1;

/// Upper bound on the worst-edge list. A patch can have far more cables than a
/// reader wants; only the slowest few are reported (spec: "Latency names the
/// worst edges"). Consumers: the `cli` renderer and `tests/cli.rs`.
const WORST_EDGE_LIMIT: usize = 10;

/// The complete `analyze` payload for one patch.
///
/// Field order is the serializer's output order. Consumed by the `cli`
/// serializer (`analyze --json`), the human text renderer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AnalyzeReport {
    /// Report contract version; see [`REPORT_VERSION`].
    pub report_version: u32,
    /// Producer identity, so a stored report names the binary that wrote it.
    pub tool: ToolInfo,
    /// What was analyzed and how big it is.
    pub patch: PatchSummary,
    /// Which schema answered the analysis (embedded or extended by plugins).
    pub schema: SchemaSummary,
    /// Resolved master model and how it was resolved.
    pub master: MasterSummary,
    /// RAM footprint against the resolved master's budget.
    pub ram: RamSummary,
    /// Validation findings, in the validator's source order.
    pub validation: Vec<ValidationFinding>,
    /// Signal-flow topology findings, in graph-build order.
    pub topology: Vec<TopologyFinding>,
    /// Forward-loop latency summary plus the slowest cables.
    pub latency: LatencyReport,
    /// Banner groups in file order, with their section ranges.
    pub groups: Vec<GroupSummary>,
}

/// Name and version of the binary that produced the report.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolInfo {
    /// `env!("CARGO_PKG_NAME")`.
    pub name: String,
    /// `env!("CARGO_PKG_VERSION")`.
    pub version: String,
}

/// Size and shape of the analyzed patch.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PatchSummary {
    /// The path as given on the command line, or `<stdin>`.
    pub path: String,
    /// Patch name from the loader (usually the file stem).
    pub name: String,
    /// Size of the source in bytes.
    pub bytes: u64,
    /// Number of parsed `[section]`s, including controllers and unknown names.
    pub sections: usize,
    /// Sections whose lowercased name is a schema circuit.
    pub circuits: usize,
    /// Virtual cables in the cable index.
    pub cables: usize,
    /// Hardware tokens collected from section parameters.
    pub hw_tokens: usize,
}

/// Provenance of the schema used for the analysis (design D10).
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SchemaSummary {
    /// Number of circuit definitions available.
    pub circuits: usize,
    /// `"embedded"` or `"embedded+plugins"`.
    pub source: String,
    /// Plugin files applied over the embedded schema, in load order.
    pub plugins: Vec<String>,
}

/// The resolved master model and why it was chosen.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MasterSummary {
    /// Human-facing model label, e.g. `MASTER18`.
    pub model: String,
    /// `"declared"`, `"inferred"` or `"default"`.
    pub source: String,
    /// Whether some model can host every use in the patch.
    pub satisfiable: bool,
}

/// RAM footprint of the patch against the resolved master's budget.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RamSummary {
    /// Sum of `ramsize` over sections that resolve to a circuit or controller.
    pub used: u32,
    /// The resolved master's `available_memory` budget, or `0` if absent.
    pub budget: u32,
    /// Human-facing model label whose budget was used.
    pub model: String,
}

/// One validation finding, flattened from the validator's span-carrying issue.
///
/// Consumed by the `cli` serializer (severity tally and findings list) and
/// `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ValidationFinding {
    /// Lowercase severity: `"error"`, `"warning"` or `"hint"`.
    pub severity: String,
    /// Stable machine code, e.g. `duplicate_param`.
    pub code: String,
    /// User-facing message.
    pub message: String,
    /// Zero-based source line.
    pub line: usize,
    /// Start byte column within the raw line.
    pub col_start: usize,
    /// End byte column (exclusive).
    pub col_end: usize,
}

/// One signal-flow topology finding, flattened from [`crate::graph::TopologyIssue`].
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TopologyFinding {
    /// Cable the finding is attached to.
    pub cable: String,
    /// Lowercase severity: `"error"` or `"warning"`.
    pub severity: String,
    /// User-facing message.
    pub message: String,
}

/// Latency summary with a bounded list of the slowest edges.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LatencyReport {
    /// Mean latency over all edges, rounded to four decimals.
    pub avg: f32,
    /// Largest edge latency, rounded to four decimals.
    pub max: f32,
    /// Number of edges that wrap the loop boundary.
    pub back_edges: usize,
    /// Slowest edges, worst first, at most `WORST_EDGE_LIMIT` entries.
    pub worst: Vec<WorstEdge>,
}

/// One slow cable with its endpoints and latency.
///
/// Consumed by the `cli` serializer (the "worst cables" section) and
/// `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorstEdge {
    /// Virtual cable name.
    pub cable: String,
    /// Producer endpoint: `<name> <1-based instance>` or a bare jack token.
    pub source: String,
    /// Consumer endpoint: `<name> <1-based instance>` or a bare jack token.
    pub sink: String,
    /// Forward-loop latency in loop units, rounded to four decimals.
    pub latency: f32,
}

/// One banner group and the section occurrence range it owns.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GroupSummary {
    /// Banner text, or `(unnamed)` for the implicit first group.
    pub title: String,
    /// First section occurrence index (inclusive).
    pub start: usize,
    /// Last section occurrence index (exclusive).
    pub end: usize,
}

/// The complete `optimize` payload: the optimizer's candidates with their
/// measured latency before and after, and the permutation each one applies.
///
/// Field order is the serializer's output order. Consumed by the `cli`
/// serializer (`optimize --json`), the human text renderer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OptimizeReport {
    /// Report contract version; see [`REPORT_VERSION`].
    pub report_version: u32,
    /// Producer identity, so a stored report names the binary that wrote it.
    pub tool: ToolInfo,
    /// What was optimized and how big it is.
    pub patch: PatchSummary,
    /// Which candidate strategy searched (`banner`, `global` or `minmax`).
    pub strategy: String,
    /// The objective weight the search used: `0.0` is total latency only,
    /// `1.0` is the single worst edge only. Recorded so two reports of one
    /// patch are comparable without the command line that produced them.
    pub weight: f32,
    /// Latency of the original file order; the `before` every candidate shares.
    pub baseline: LatencySummaryReport,
    /// Candidate orderings, best first (the optimizer's own order).
    pub candidates: Vec<CandidateReport>,
}

/// One candidate ordering with its measured effect.
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CandidateReport {
    /// Human-readable strategy label, e.g. `"banner (default)"`.
    pub label: String,
    /// Latency of the original file order.
    pub before: LatencySummaryReport,
    /// Latency of the reordered file.
    pub after: LatencySummaryReport,
    /// `after.avg − before.avg`, rounded to four decimals.
    pub delta_avg: f32,
    /// `after.max − before.max`, rounded to four decimals.
    pub delta_max: f32,
    /// Permutation of section indices: `order[i]` is the section at file
    /// position `i` in the candidate.
    pub order: Vec<usize>,
    /// File the CLI saved this candidate to via `--write`, or `None` when it
    /// was not written. Always `None` from [`optimize_report`].
    pub wrote: Option<String>,
}

/// Compact latency summary for a candidate (no per-edge list).
///
/// Consumed by the `cli` serializer and `tests/cli.rs`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LatencySummaryReport {
    /// Mean latency over all edges, rounded to four decimals.
    pub avg: f32,
    /// Largest edge latency, rounded to four decimals.
    pub max: f32,
    /// Number of edges that wrap the loop boundary.
    pub back_edges: usize,
}

/// Build the optimization report for one patch and its already-generated
/// candidates.
///
/// The caller owns candidate generation (and with it the strategy and search
/// weight): this function neither runs the optimizer nor measures latency, it
/// only projects the measured [`CandidateOrdering`]s into the report contract.
/// `schema` is the same schema the caller fed the optimizer and the graph build,
/// so the circuit count in [`PatchSummary`] cannot disagree with the analysis
/// report of the same patch.
///
/// `path` is the label the CLI wants to show (`<stdin>` for standard input) and
/// `bytes` the source size. Every reported float is rounded through `round4`,
/// and the deltas are computed from the rounded `before`/`after` values, so the
/// printed numbers satisfy `after − before == delta` consistently.
///
/// Pure and deterministic. Consumers: the `cli` module's `optimize` dispatch and
/// the unit tests below.
pub fn optimize_report(
    patch: &Patch,
    schema: &Schema,
    candidates: &[CandidateOrdering],
    strategy: OptimizeScope,
    weight: f32,
    path: &str,
    bytes: u64,
) -> OptimizeReport {
    // Invariant: every candidate of one patch carries the same `before` (the
    // original file order's summary), so the first candidate's value is the
    // baseline. Documented rather than asserted: a caller that violates it gets
    // a report labelled with the first candidate's baseline, never a panic.
    let baseline = candidates.first().map_or(
        LatencySummaryReport {
            avg: 0.0,
            max: 0.0,
            back_edges: 0,
        },
        |candidate| latency_summary_report(&candidate.before),
    );

    OptimizeReport {
        report_version: REPORT_VERSION,
        tool: ToolInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        patch: patch_summary(patch, schema, path, bytes),
        strategy: strategy_label(strategy).to_string(),
        weight: round4(weight),
        baseline,
        candidates: candidates
            .iter()
            .map(|candidate| {
                let before = latency_summary_report(&candidate.before);
                let after = latency_summary_report(&candidate.after);
                CandidateReport {
                    label: candidate.label.clone(),
                    // Deltas come from the rounded values, not the raw
                    // summaries, so `after − before` always reproduces them.
                    delta_avg: round4(after.avg - before.avg),
                    delta_max: round4(after.max - before.max),
                    before,
                    after,
                    order: candidate.order.clone(),
                    wrote: None,
                }
            })
            .collect(),
    }
}

/// Build the analysis report for one patch.
///
/// `graph` is the caller's already-built signal-flow graph, so this function
/// never rebuilds or re-validates topology. `path` is the label the CLI wants
/// to show (`<stdin>` for standard input) and `bytes` the source size; `plugins`
/// lists applied plugin files in load order.
///
/// Pure and deterministic: it reads the patch, schema and graph and returns
/// owned data. Consumers: the `cli` module's `analyze` dispatch and the
/// unit tests below.
pub fn analyze_report(
    patch: &Patch,
    schema: &Schema,
    graph: &Graph,
    path: &str,
    bytes: u64,
    plugins: &[String],
) -> AnalyzeReport {
    let plan = rack::resolve(patch, schema);

    AnalyzeReport {
        report_version: REPORT_VERSION,
        tool: ToolInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        patch: patch_summary(patch, schema, path, bytes),
        schema: SchemaSummary {
            circuits: schema.circuits.len(),
            source: if plugins.is_empty() {
                String::from("embedded")
            } else {
                String::from("embedded+plugins")
            },
            plugins: plugins.to_vec(),
        },
        master: MasterSummary {
            model: plan.model.label().to_string(),
            source: master_source_label(plan.source).to_string(),
            satisfiable: plan.satisfiable,
        },
        ram: RamSummary {
            used: ram_used(patch, schema),
            budget: schema
                .available_memory
                .get(plan.model.ram_key())
                .copied()
                .unwrap_or(0),
            model: plan.model.label().to_string(),
        },
        validation: validation::validate_patch(patch, schema)
            .into_iter()
            .map(|issue| ValidationFinding {
                severity: severity_label(issue.severity).to_string(),
                code: issue.code,
                message: issue.message,
                line: issue.span.line,
                col_start: issue.span.col_start,
                col_end: issue.span.col_end,
            })
            .collect(),
        topology: graph
            .validation
            .iter()
            .map(|issue| TopologyFinding {
                cable: issue.cable.clone(),
                severity: topology_severity_label(issue.severity).to_string(),
                message: issue.message.clone(),
            })
            .collect(),
        latency: latency_report(graph),
        groups: patch
            .banner_groups
            .iter()
            .map(|group| GroupSummary {
                title: group.banner.as_deref().unwrap_or("(unnamed)").to_string(),
                start: group.section_range.start,
                end: group.section_range.end,
            })
            .collect(),
    }
}

/// The latency section: zeros when the graph has nothing to measure, otherwise
/// the rounded summary plus the capped worst-edge list.
fn latency_report(graph: &Graph) -> LatencyReport {
    let Some(data) = &graph.latency else {
        return LatencyReport {
            avg: 0.0,
            max: 0.0,
            back_edges: 0,
            worst: Vec::new(),
        };
    };

    LatencyReport {
        avg: round4(data.summary.avg),
        max: round4(data.summary.max),
        back_edges: data.summary.back_edge_count,
        worst: worst_edges(&data.edges, &graph.edges),
    }
}

/// Pair each latency entry with its edge, order worst first, then keep the
/// bounded head.
///
/// Latency is compared after rounding, so the reported order always matches
/// the reported numbers. Ties fall back to cable, then source, then sink
/// (all ascending) for a total, deterministic order.
fn worst_edges(entries: &[EdgeLatency], edges: &[GraphEdge]) -> Vec<WorstEdge> {
    let mut worst: Vec<WorstEdge> = entries
        .iter()
        .filter_map(|entry| {
            let edge = edges.get(entry.edge_index)?;
            Some(WorstEdge {
                cable: edge.cable.clone(),
                source: format_node(&edge.source),
                sink: format_node(&edge.sink),
                latency: round4(entry.latency),
            })
        })
        .collect();

    worst.sort_by(|a, b| {
        b.latency
            .total_cmp(&a.latency)
            .then_with(|| a.cable.cmp(&b.cable))
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.sink.cmp(&b.sink))
    });
    worst.truncate(WORST_EDGE_LIMIT);
    worst
}

/// Endpoint label used by the report: `<name> <1-based instance>` for circuits
/// and controllers, the bare token for a jack.
fn format_node(id: &NodeId) -> String {
    match id {
        NodeId::Circuit(name, instance) | NodeId::Controller(name, instance) => {
            format!("{name} {}", instance + 1)
        }
        NodeId::Jack(token) => token.clone(),
    }
}

/// The shared patch projection used by every report: path, loader name, byte
/// size and the counts both builders report.
fn patch_summary(patch: &Patch, schema: &Schema, path: &str, bytes: u64) -> PatchSummary {
    PatchSummary {
        path: path.to_string(),
        name: patch.name.clone(),
        bytes,
        sections: patch.sections.len(),
        circuits: patch
            .sections
            .iter()
            .filter(|section| schema.circuits.contains_key(&section.name.to_lowercase()))
            .count(),
        cables: patch.cable_index.len(),
        hw_tokens: patch.hw_components.len(),
    }
}

/// Project a measured [`LatencySummary`] onto the report's rounded shape.
fn latency_summary_report(summary: &LatencySummary) -> LatencySummaryReport {
    LatencySummaryReport {
        avg: round4(summary.avg),
        max: round4(summary.max),
        back_edges: summary.back_edge_count,
    }
}

/// RAM footprint: sum `ramsize` over sections that resolve to a circuit or a
/// controller, mirroring the rule in [`validation::validate_patch`].
fn ram_used(patch: &Patch, schema: &Schema) -> u32 {
    let mut used: u32 = 0;
    for section in &patch.sections {
        let name = section.name.to_lowercase();
        if let Some(circuit) = schema.circuits.get(&name) {
            used = used.saturating_add(circuit.ramsize);
        } else if let Some(controller) = schema.controllers.get(&name) {
            used = used.saturating_add(controller.ramsize);
        }
    }
    used
}

/// The strategy name reported for a search scope.
fn strategy_label(scope: OptimizeScope) -> &'static str {
    match scope {
        OptimizeScope::Banner => "banner",
        OptimizeScope::Global => "global",
        OptimizeScope::MinMax => "minmax",
    }
}

/// Serializer-facing severity name for a validation finding.
fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Hint => "hint",
    }
}

/// Serializer-facing severity name for a topology finding.
fn topology_severity_label(severity: TopologySeverity) -> &'static str {
    match severity {
        TopologySeverity::Error => "error",
        TopologySeverity::Warning => "warning",
    }
}

/// Serializer-facing name for where the master model came from.
fn master_source_label(source: MasterSource) -> &'static str {
    match source {
        MasterSource::Declared => "declared",
        MasterSource::Inferred => "inferred",
        MasterSource::Default => "default",
    }
}

/// Round to four decimals — the report's single latency precision rule.
fn round4(value: f32) -> f32 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{clusters_from_patch, GraphNode, GraphOptions, NodeKind};
    use crate::latency::{CostModel, LatencyData, LatencySummary};
    use crate::optimize::{generate_candidates, OptimizeScope};
    use crate::schema::load_schema;

    /// A hand-built patch with no sections, cables or hardware tokens. `sample()`
    /// already carries empty collections; only its demo name and hardware
    /// components are replaced.
    fn empty_patch() -> Patch {
        Patch {
            name: String::from("empty.ini"),
            hw_components: Vec::new(),
            ..Patch::sample()
        }
    }

    /// A graph whose latency data was produced from `latencies`, one entry per
    /// edge in the same order.
    fn graph_with_latency(
        edges: Vec<GraphEdge>,
        latencies: Vec<f32>,
        summary: LatencySummary,
    ) -> Graph {
        let entries = latencies
            .into_iter()
            .enumerate()
            .map(|(edge_index, latency)| EdgeLatency {
                edge_index,
                latency,
                is_back_edge: false,
            })
            .collect();
        Graph {
            edges,
            latency: Some(LatencyData {
                edges: entries,
                summary,
            }),
            ..Graph::default()
        }
    }

    /// One circuit node — only the id matters for endpoint formatting.
    fn circuit_node(name: &str, instance: usize) -> GraphNode {
        GraphNode {
            id: NodeId::circuit(name, instance),
            kind: NodeKind::Circuit,
            circuit: name.to_string(),
            instance_index: instance,
            section_index: instance,
        }
    }

    #[test]
    fn empty_patch_reports_version_and_zero_counts() {
        let patch = empty_patch();
        let schema = crate::test_support::catalog_schema();
        let graph = Graph::default();

        let report = analyze_report(&patch, schema, &graph, "<stdin>", 0, &[]);

        assert_eq!(report.report_version, REPORT_VERSION);
        assert_eq!(report.report_version, 1);
        assert_eq!(report.tool.name, env!("CARGO_PKG_NAME"));
        assert_eq!(report.tool.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(report.patch.path, "<stdin>");
        assert_eq!(report.patch.name, "empty.ini");
        assert_eq!(report.patch.bytes, 0);
        assert_eq!(report.patch.sections, 0);
        assert_eq!(report.patch.circuits, 0);
        assert_eq!(report.patch.cables, 0);
        assert_eq!(report.patch.hw_tokens, 0);
        assert_eq!(report.schema.circuits, schema.circuits.len());
        assert_eq!(report.schema.source, "embedded");
        assert_eq!(report.schema.plugins.len(), 0);
        assert_eq!(report.master.model, "MASTER");
        assert_eq!(report.master.source, "default");
        assert!(report.master.satisfiable);
        assert_eq!(report.ram.used, 0);
        assert_eq!(report.ram.budget, 0);
        assert_eq!(report.ram.model, "MASTER");
        assert_eq!(report.validation.len(), 0);
        assert_eq!(report.topology.len(), 0);
        assert_eq!(report.latency.avg, 0.0);
        assert_eq!(report.latency.max, 0.0);
        assert_eq!(report.latency.back_edges, 0);
        assert_eq!(report.latency.worst.len(), 0);
        assert_eq!(report.groups.len(), 0);
    }

    #[test]
    fn plugin_list_changes_schema_provenance() {
        let patch = empty_patch();
        let schema = crate::test_support::catalog_schema();
        let plugins = vec![String::from("extra.json")];

        let report = analyze_report(&patch, schema, &Graph::default(), "p.ini", 3, &plugins);

        assert_eq!(report.schema.source, "embedded+plugins");
        assert_eq!(report.schema.plugins, plugins);
        assert_eq!(report.patch.bytes, 3);
    }

    #[test]
    fn finding_fixture_preserves_source_order_and_spans() {
        // Duplicate `input` on line 2, unknown `notaparam` on line 3: the
        // validator sorts by (line, col), so the duplicate must come first.
        let ini = "[copy]\ninput = I1\ninput = I2\nnotaparam = 5\noutput = O1\n";
        let patch = Patch::from_ini_str(ini, String::from("dup.ini")).expect("fixture parses");
        let schema = load_schema();
        let graph =
            Graph::build_from_patch(&patch, &[], &CostModel::default(), &GraphOptions::default());

        let report = analyze_report(&patch, schema, &graph, "dup.ini", ini.len() as u64, &[]);

        // The projection must not reorder relative to the validator.
        let expected: Vec<(String, usize, usize, usize)> =
            validation::validate_patch(&patch, schema)
                .into_iter()
                .map(|issue| {
                    (
                        issue.code,
                        issue.span.line,
                        issue.span.col_start,
                        issue.span.col_end,
                    )
                })
                .collect();
        let got: Vec<(String, usize, usize, usize)> = report
            .validation
            .iter()
            .map(|finding| {
                (
                    finding.code.clone(),
                    finding.line,
                    finding.col_start,
                    finding.col_end,
                )
            })
            .collect();
        assert_eq!(got, expected);

        let duplicate = report
            .validation
            .iter()
            .find(|finding| finding.code == "duplicate_param")
            .expect("duplicate_param finding");
        assert_eq!(duplicate.severity, "warning");
        assert_eq!(duplicate.line, 2);
        assert_eq!((duplicate.col_start, duplicate.col_end), (8, 10));
        assert!(duplicate.message.contains("input"));

        let duplicate_at = report
            .validation
            .iter()
            .position(|finding| finding.code == "duplicate_param")
            .expect("duplicate present");
        let unknown_at = report
            .validation
            .iter()
            .position(|finding| finding.code == "unknown_param")
            .expect("unknown_param present");
        assert!(duplicate_at < unknown_at, "{:?}", report.validation);
    }

    #[test]
    fn round4_keeps_exactly_four_decimals() {
        assert_eq!(round4(0.123_456), 0.1235);
        assert_eq!(round4(1.0 / 3.0), 0.3333);
        assert_eq!(round4(2.000_04), 2.0);
        assert_eq!(round4(0.0), 0.0);
    }

    #[test]
    fn latency_summary_is_rounded() {
        let graph = graph_with_latency(
            vec![GraphEdge {
                cable: String::from("_C"),
                source: NodeId::circuit("src", 0),
                sink: NodeId::circuit("sink", 0),
            }],
            vec![3.456_78],
            LatencySummary {
                avg: 1.234_56,
                max: 2.345_67,
                back_edge_count: 2,
            },
        );

        let report = analyze_report(
            &empty_patch(),
            crate::test_support::catalog_schema(),
            &graph,
            "r.ini",
            0,
            &[],
        );

        assert_eq!(report.latency.avg, 1.2346);
        assert_eq!(report.latency.max, 2.3457);
        assert_eq!(report.latency.back_edges, 2);
        assert_eq!(report.latency.worst.len(), 1);
        assert_eq!(report.latency.worst[0].latency, 3.4568);
    }

    #[test]
    fn worst_edges_are_capped_sorted_and_cable_tie_broken() {
        // 12 cables, 12 latencies; `c08`/`c09` share 9.0 to exercise the tie.
        let cables: Vec<String> = (0..12).map(|i| format!("c{i:02}")).collect();
        let edges: Vec<GraphEdge> = cables
            .iter()
            .map(|cable| GraphEdge {
                cable: cable.clone(),
                source: NodeId::circuit("src", 0),
                sink: NodeId::circuit("sink", 0),
            })
            .collect();
        let latencies = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 9.0, 10.0, 11.0];
        let graph = graph_with_latency(
            edges,
            latencies,
            LatencySummary {
                avg: 0.0,
                max: 0.0,
                back_edge_count: 0,
            },
        );

        let report = analyze_report(
            &empty_patch(),
            crate::test_support::catalog_schema(),
            &graph,
            "worst.ini",
            0,
            &[],
        );

        let worst = &report.latency.worst;
        assert_eq!(worst.len(), WORST_EDGE_LIMIT);
        let order: Vec<&str> = worst.iter().map(|edge| edge.cable.as_str()).collect();
        assert_eq!(
            order,
            vec!["c11", "c10", "c08", "c09", "c07", "c06", "c05", "c04", "c03", "c02"]
        );
        let values: Vec<f32> = worst.iter().map(|edge| edge.latency).collect();
        assert_eq!(
            values,
            vec![11.0, 10.0, 9.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0]
        );
    }

    #[test]
    fn worst_edges_tie_break_by_endpoint_and_format_instances() {
        // All three edges: same cable, same latency. Endpoint strings decide.
        let edges = vec![
            GraphEdge {
                cable: String::from("_TIE"),
                source: NodeId::circuit("b", 0),
                sink: NodeId::circuit("out", 0),
            },
            GraphEdge {
                cable: String::from("_TIE"),
                source: NodeId::circuit("a", 0),
                sink: NodeId::circuit("out", 0),
            },
            GraphEdge {
                cable: String::from("_TIE"),
                source: NodeId::Jack(String::from("I1")),
                sink: NodeId::circuit("out", 0),
            },
        ];
        let graph = graph_with_latency(
            edges,
            vec![5.0, 5.0, 5.0],
            LatencySummary {
                avg: 5.0,
                max: 5.0,
                back_edge_count: 0,
            },
        );

        let report = analyze_report(
            &empty_patch(),
            crate::test_support::catalog_schema(),
            &graph,
            "tie.ini",
            0,
            &[],
        );

        let sources: Vec<&str> = report
            .latency
            .worst
            .iter()
            .map(|edge| edge.source.as_str())
            .collect();
        assert_eq!(sources, vec!["I1", "a 1", "b 1"]);
        assert!(report.latency.worst.iter().all(|edge| edge.sink == "out 1"));
    }

    #[test]
    fn graph_nodes_are_unused_by_the_report_but_keep_endpoint_refs_valid() {
        // Endpoint labels come from edge ids, not the node list; keep a node list
        // to mirror a real graph and prove no lookup panic occurs.
        let graph = Graph {
            nodes: vec![circuit_node("src", 0), circuit_node("sink", 0)],
            edges: vec![GraphEdge {
                cable: String::from("_C"),
                source: NodeId::circuit("src", 0),
                sink: NodeId::circuit("sink", 0),
            }],
            ..Graph::default()
        };

        let report = analyze_report(
            &empty_patch(),
            crate::test_support::catalog_schema(),
            &graph,
            "n.ini",
            0,
            &[],
        );

        assert_eq!(report.latency.worst.len(), 0);
    }

    /// A reorderable four-section chain: file order d, b, a, c wired
    /// a→c→b→d. The optimizer's unique optimum is the chain order
    /// `[2, 3, 0, 1]`; no banners, so the whole file is one search domain.
    fn chain_patch() -> Patch {
        Patch::from_ini_str(
            "[d]\n\
             button = B1.1\n\
             in = _BD\n\
             [b]\n\
             output = _BD\n\
             in = _CB\n\
             [a]\n\
             output = _AC\n\
             [c]\n\
             output = _CB\n\
             in = _AC\n",
            String::from("chain.ini"),
        )
        .expect("fixture parses")
    }

    fn identity_order(len: usize) -> Vec<usize> {
        (0..len).collect()
    }

    /// Latency summary of `patch` exactly as the graph build measures it.
    fn measured_summary(patch: &Patch) -> LatencySummary {
        let graph = Graph::build_from_patch(
            patch,
            &clusters_from_patch(patch),
            &CostModel::default(),
            &GraphOptions::default(),
        );
        graph.latency.expect("fixture has cables").summary
    }

    #[test]
    fn optimize_report_projects_candidates_and_permutations() {
        let patch = chain_patch();
        let cost = CostModel::default();
        let candidates = generate_candidates(&patch, &cost, OptimizeScope::Banner);
        assert!(
            !candidates.is_empty(),
            "the reorderable fixture must yield a candidate"
        );

        let report = optimize_report(
            &patch,
            load_schema(),
            &candidates,
            OptimizeScope::Banner,
            0.0,
            "chain.ini",
            123,
        );

        assert_eq!(report.report_version, REPORT_VERSION);
        assert_eq!(report.tool.name, env!("CARGO_PKG_NAME"));
        assert_eq!(report.patch.path, "chain.ini");
        assert_eq!(report.patch.name, "chain.ini");
        assert_eq!(report.patch.bytes, 123);
        assert_eq!(report.patch.sections, patch.sections.len());
        assert_eq!(report.patch.cables, patch.cable_index.len());
        assert_eq!(report.candidates.len(), candidates.len());

        // The baseline is the first candidate's `before`, and that is the
        // measured latency of the original file order.
        let measured_before = measured_summary(&patch);
        assert_eq!(report.baseline, report.candidates[0].before);
        assert_eq!(report.baseline.avg, round4(measured_before.avg));
        assert_eq!(report.baseline.max, round4(measured_before.max));
        assert_eq!(report.baseline.back_edges, measured_before.back_edge_count);

        for (candidate, raw) in report.candidates.iter().zip(&candidates) {
            assert_eq!(candidate.label, raw.label);
            assert_eq!(candidate.order, raw.order);
            assert!(candidate.wrote.is_none());
            // `order` is a permutation of every section index.
            let mut sorted = candidate.order.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, identity_order(patch.sections.len()));
            assert_eq!(candidate.before, report.baseline);
        }

        // At least one candidate actually reorders this fixture, and it is an
        // improvement over the baseline.
        let reordered = report
            .candidates
            .iter()
            .find(|candidate| candidate.order != identity_order(patch.sections.len()))
            .expect("a reorderable fixture yields a reordering candidate");
        assert!(reordered.delta_avg < 0.0);
    }

    #[test]
    fn optimize_report_after_matches_measured_latency_of_the_permutation() {
        let patch = chain_patch();
        // One implicit banner group over the whole file: reordering sections
        // cannot invalidate a group range.
        assert_eq!(patch.banner_groups.len(), 1);
        assert_eq!(
            patch.banner_groups[0].section_range,
            0..patch.sections.len()
        );
        let cost = CostModel::default();
        let candidates = generate_candidates(&patch, &cost, OptimizeScope::MinMax);
        let report = optimize_report(
            &patch,
            load_schema(),
            &candidates,
            OptimizeScope::MinMax,
            0.0,
            "chain.ini",
            0,
        );

        for candidate in &report.candidates {
            let mut reordered = patch.clone();
            reordered.sections = candidate
                .order
                .iter()
                .map(|&index| patch.sections[index].clone())
                .collect();
            let measured = measured_summary(&reordered);
            assert_eq!(
                candidate.after.avg,
                round4(measured.avg),
                "avg of {:?}",
                candidate.label
            );
            assert_eq!(
                candidate.after.max,
                round4(measured.max),
                "max of {:?}",
                candidate.label
            );
            assert_eq!(
                candidate.after.back_edges, measured.back_edge_count,
                "back edges of {:?}",
                candidate.label
            );
        }
    }

    #[test]
    fn optimize_report_deltas_come_from_rounded_summaries() {
        let patch = chain_patch();
        let cost = CostModel::default();
        let candidates = generate_candidates(&patch, &cost, OptimizeScope::Banner);
        let report = optimize_report(
            &patch,
            load_schema(),
            &candidates,
            OptimizeScope::Banner,
            0.0,
            "chain.ini",
            0,
        );

        for (candidate, raw) in report.candidates.iter().zip(&candidates) {
            // The reported delta is exactly the difference of the reported
            // (already rounded) before/after values.
            assert_eq!(
                candidate.delta_avg,
                round4(candidate.after.avg - candidate.before.avg)
            );
            assert_eq!(
                candidate.delta_max,
                round4(candidate.after.max - candidate.before.max)
            );
            // ... and agrees with the raw measurements to four decimals.
            assert_eq!(
                (candidate.delta_avg * 10_000.0).round() as i64,
                ((raw.after.avg - raw.before.avg) * 10_000.0).round() as i64
            );
            assert_eq!(
                (candidate.delta_max * 10_000.0).round() as i64,
                ((raw.after.max - raw.before.max) * 10_000.0).round() as i64
            );
        }
    }

    #[test]
    fn optimize_report_without_candidates_zeroes_the_baseline() {
        let patch = chain_patch();
        let report = optimize_report(
            &patch,
            load_schema(),
            &[],
            OptimizeScope::Banner,
            0.0,
            "<stdin>",
            0,
        );

        assert_eq!(
            report.baseline,
            LatencySummaryReport {
                avg: 0.0,
                max: 0.0,
                back_edges: 0,
            }
        );
        assert!(report.candidates.is_empty());
        assert_eq!(report.patch.path, "<stdin>");
        assert_eq!(report.patch.sections, patch.sections.len());
    }
}
