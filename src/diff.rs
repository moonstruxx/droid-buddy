use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::patch::{NodeId, Patch};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangedCable {
    pub cable: String,
    pub old_sources: Vec<String>,
    pub new_sources: Vec<String>,
    pub old_sinks: Vec<(NodeId, String)>,
    pub new_sinks: Vec<(NodeId, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangedNode {
    pub id: NodeId,
    pub changed_params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffReport {
    pub added_cables: Vec<String>,
    pub removed_cables: Vec<String>,
    pub changed_cables: Vec<ChangedCable>,
    pub added_nodes: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
    pub changed_nodes: Vec<ChangedNode>,
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn build_node_ids(sections: &[crate::patch::IniSection]) -> Vec<NodeId> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(sections.len());
    for s in sections {
        let c = counts.entry(s.name.clone()).or_insert(0);
        out.push((s.name.clone(), *c));
        *c += 1;
    }
    out
}

fn section_name_to_node_ids(patch: &Patch) -> HashMap<String, Vec<NodeId>> {
    let ids = build_node_ids(&patch.sections);
    let mut m: HashMap<String, Vec<NodeId>> = HashMap::new();
    for nid in ids {
        m.entry(nid.0.clone()).or_default().push(nid);
    }
    for v in m.values_mut() {
        v.sort();
    }
    m
}

fn is_cable_value(v: &str) -> bool {
    let t = v.trim();
    t.starts_with('_')
        && t.len() > 1
        && t.chars()
            .skip(1)
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn resolve_sinks(
    entry: &crate::patch::CableIndexEntry,
    name_to_ids: &HashMap<String, Vec<NodeId>>,
) -> BTreeSet<(NodeId, String)> {
    let mut out = BTreeSet::new();
    for (sink_circuit, sink_param) in &entry.sink_refs {
        if let Some(ids) = name_to_ids.get(sink_circuit) {
            for nid in ids {
                out.insert((nid.clone(), sink_param.clone()));
            }
        } else {
            // fallback: circuit not found as section (e.g. preamble) — use sentinel
            out.insert(((sink_circuit.clone(), 0), sink_param.clone()));
        }
    }
    out
}

fn node_settings(patch: &Patch) -> HashMap<NodeId, BTreeMap<String, String>> {
    let ids = build_node_ids(&patch.sections);
    let mut map: HashMap<NodeId, BTreeMap<String, String>> = HashMap::new();
    for (sec, nid) in patch.sections.iter().zip(ids.iter()) {
        let e = map.entry(nid.clone()).or_default();
        for (k, v) in &sec.entries {
            if is_cable_value(v) {
                continue;
            }
            // skip values that look like _CABLE expression? keep simple: exact _NAME only
            // is what collect_cable_index treats as cable; follow same rule
            e.insert(k.clone(), v.clone());
        }
    }
    map
}

// ---------------------------------------------------------------------------
// scope filtering
// ---------------------------------------------------------------------------

/// Filter a [`DiffReport`] to only those cables/nodes that intersect the
/// selected hardware token's influence subtree.
///
/// Influence is derived via `Patch::hw_token_to_vars` (boundary-aware) and
/// `Patch::influence_subtree` (transitive forward BFS over `cable_index` +
/// `circuit_outputs`, cycle-safe).  A diff entry is retained when it touches
/// an influenced cable or an influenced/originating circuit instance.
/// Returns sorted, deterministic outputs.  An unknown token yields an empty
/// report.
pub fn scope_report(report: &DiffReport, token: &str, patch: &Patch) -> DiffReport {
    let vars = patch.hw_token_to_vars(token);
    if vars.is_empty() {
        return DiffReport::default();
    }
    let subtree = patch.influence_subtree(&vars);
    let influenced_edges = &subtree.influenced_edges;
    let influenced_nodes = &subtree.influenced_nodes;

    // NodeIds of sections that directly contain the token — the originating
    // circuits.  They stay in scope even though `influence_subtree` only marks
    // downstream sinks, not the sources.
    let node_ids = build_node_ids(&patch.sections);
    let mut token_nodes: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    for (sec, nid) in patch.sections.iter().zip(node_ids.iter()) {
        let has_token = sec
            .entries
            .iter()
            .any(|(_, v)| crate::patch::scan_hw_tokens(v).iter().any(|t| t == token));
        if has_token {
            token_nodes.insert(nid.clone());
        }
    }
    // Also include hw_components whose id == token?  `scan_hw_tokens` already
    // covers param values, but a token may appear as a bare `button = B1.1`
    // which is captured there too.  No extra hw_components scan needed.

    let mut added_cables: Vec<String> = report
        .added_cables
        .iter()
        .filter(|c| influenced_edges.contains(*c))
        .cloned()
        .collect();
    let mut removed_cables: Vec<String> = report
        .removed_cables
        .iter()
        .filter(|c| influenced_edges.contains(*c))
        .cloned()
        .collect();
    let mut changed_cables: Vec<ChangedCable> = report
        .changed_cables
        .iter()
        .filter(|cc| influenced_edges.contains(&cc.cable))
        .cloned()
        .collect();

    let mut added_nodes: Vec<NodeId> = report
        .added_nodes
        .iter()
        .filter(|nid| influenced_nodes.contains(*nid) || token_nodes.contains(*nid))
        .cloned()
        .collect();
    let mut removed_nodes: Vec<NodeId> = report
        .removed_nodes
        .iter()
        .filter(|nid| influenced_nodes.contains(*nid) || token_nodes.contains(*nid))
        .cloned()
        .collect();
    let mut changed_nodes: Vec<ChangedNode> = report
        .changed_nodes
        .iter()
        .filter(|cn| influenced_nodes.contains(&cn.id) || token_nodes.contains(&cn.id))
        .cloned()
        .collect();

    added_cables.sort();
    removed_cables.sort();
    changed_cables.sort();
    added_nodes.sort();
    removed_nodes.sort();
    changed_nodes.sort();

    DiffReport {
        added_cables,
        removed_cables,
        changed_cables,
        added_nodes,
        removed_nodes,
        changed_nodes,
    }
}

// ---------------------------------------------------------------------------
// public
// ---------------------------------------------------------------------------

pub fn diff_patches(a: &Patch, b: &Patch) -> DiffReport {
    // cable diff
    let a_name_to_ids = section_name_to_node_ids(a);
    let b_name_to_ids = section_name_to_node_ids(b);

    let a_cables: BTreeSet<String> = a.cable_index.keys().cloned().collect();
    let b_cables: BTreeSet<String> = b.cable_index.keys().cloned().collect();

    let mut added_cables: Vec<String> = b_cables.difference(&a_cables).cloned().collect();
    let mut removed_cables: Vec<String> = a_cables.difference(&b_cables).cloned().collect();
    added_cables.sort();
    removed_cables.sort();

    let mut changed_cables = Vec::new();
    for cable in a_cables.intersection(&b_cables) {
        let ae = &a.cable_index[cable];
        let be = &b.cable_index[cable];

        let a_sources: BTreeSet<String> = ae.sources.iter().cloned().collect();
        let b_sources: BTreeSet<String> = be.sources.iter().cloned().collect();

        let a_sinks = resolve_sinks(ae, &a_name_to_ids);
        let b_sinks = resolve_sinks(be, &b_name_to_ids);

        if a_sources != b_sources || a_sinks != b_sinks {
            let mut old_sources: Vec<String> = a_sources.into_iter().collect();
            let mut new_sources: Vec<String> = b_sources.into_iter().collect();
            old_sources.sort();
            new_sources.sort();
            let old_sinks: Vec<(NodeId, String)> = a_sinks.into_iter().collect();
            let new_sinks: Vec<(NodeId, String)> = b_sinks.into_iter().collect();
            changed_cables.push(ChangedCable {
                cable: cable.clone(),
                old_sources,
                new_sources,
                old_sinks,
                new_sinks,
            });
        }
    }
    changed_cables.sort();

    // node diff — key by NodeId
    let a_ids: BTreeSet<NodeId> = build_node_ids(&a.sections).into_iter().collect();
    let b_ids: BTreeSet<NodeId> = build_node_ids(&b.sections).into_iter().collect();

    let mut added_nodes: Vec<NodeId> = b_ids.difference(&a_ids).cloned().collect();
    let mut removed_nodes: Vec<NodeId> = a_ids.difference(&b_ids).cloned().collect();
    added_nodes.sort();
    removed_nodes.sort();

    let a_settings = node_settings(a);
    let b_settings = node_settings(b);

    let mut changed_nodes = Vec::new();
    for nid in a_ids.intersection(&b_ids) {
        let am = a_settings.get(nid);
        let bm = b_settings.get(nid);
        // both present or missing (empty)
        let a_map = am.cloned().unwrap_or_default();
        let b_map = bm.cloned().unwrap_or_default();
        if a_map == b_map {
            continue;
        }
        let all_keys: BTreeSet<String> = a_map.keys().chain(b_map.keys()).cloned().collect();
        let mut diff_keys = Vec::new();
        for k in all_keys {
            if a_map.get(&k) != b_map.get(&k) {
                diff_keys.push(k);
            }
        }
        diff_keys.sort();
        changed_nodes.push(ChangedNode {
            id: nid.clone(),
            changed_params: diff_keys,
        });
    }
    changed_nodes.sort();

    // HwComponent diff is intentionally not a separate report field — it is
    // order-independent by construction (keyed by HwComponent.id elsewhere).
    // Reordered HW blocks already compare equal via NodeId/cable keying; a
    // pure HwComponent delta would surface as a node param delta if it ever
    // mattered. No extra handling needed for current spec.

    DiffReport {
        added_cables,
        removed_cables,
        changed_cables,
        added_nodes,
        removed_nodes,
        changed_nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Patch;

    fn p(ini: &str) -> Patch {
        Patch::from_ini_str(ini, "t".to_string()).unwrap()
    }

    #[test]
    fn reordered_panels_equal() {
        let a = p("[p2b8]\n\
             [lfo]\n\
             button = B1.1\n\
             rate = 0.5\n\
             [env]\n\
             button = B1.2\n\
             decay = 0.2\n");
        let b = p("[p2b8]\n\
             [env]\n\
             button = B1.2\n\
             decay = 0.2\n\
             [lfo]\n\
             button = B1.1\n\
             rate = 0.5\n");
        let r = diff_patches(&a, &b);
        assert!(
            r.added_cables.is_empty(),
            "reordered panels must not add cables"
        );
        assert!(r.removed_cables.is_empty());
        assert!(r.changed_cables.is_empty());
        assert!(r.added_nodes.is_empty());
        assert!(r.removed_nodes.is_empty());
        assert!(r.changed_nodes.is_empty());
        // symmetry
        let r2 = diff_patches(&b, &a);
        assert_eq!(r, r2);
    }

    #[test]
    fn added_node_detected() {
        let base = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n");
        let extended =
            p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n[env]\nbutton = B1.2\ndecay = 0.2\n");
        let r = diff_patches(&base, &extended);
        assert_eq!(r.added_nodes, vec![("env".to_string(), 0)]);
        assert!(r.removed_nodes.is_empty());
        assert!(r.changed_nodes.is_empty());
    }

    #[test]
    fn removed_node_detected() {
        let base =
            p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n[env]\nbutton = B1.2\ndecay = 0.2\n");
        let trimmed = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n");
        let r = diff_patches(&base, &trimmed);
        assert!(r.added_nodes.is_empty());
        assert_eq!(r.removed_nodes, vec![("env".to_string(), 0)]);
        assert!(r.changed_nodes.is_empty());
    }

    #[test]
    fn cable_added_detected() {
        // A has no cable, B produces and consumes _CLK
        let a = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n");
        let b = p("[p2b8]\n\
             [lfo]\n\
             button = B1.1\n\
             output = _CLK\n\
             [env]\n\
             button = B1.2\n\
             input = _CLK\n");
        let r = diff_patches(&a, &b);
        assert_eq!(r.added_cables, vec!["_CLK".to_string()]);
        assert!(r.removed_cables.is_empty());
        assert!(r.changed_cables.is_empty());
    }

    #[test]
    fn cable_removed_detected() {
        let a = p("[p2b8]\n\
             [lfo]\n\
             button = B1.1\n\
             output = _CLK\n\
             [env]\n\
             button = B1.2\n\
             input = _CLK\n");
        let b = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n");
        let r = diff_patches(&a, &b);
        assert!(r.added_cables.is_empty());
        assert_eq!(r.removed_cables, vec!["_CLK".to_string()]);
    }

    #[test]
    fn cable_sink_changed_reported_as_changed_cable() {
        // Same cable _CLK in both, but A feeds lfo, B feeds env
        let a = p("[p2b8]\n\
             [clock]\n\
             button = B1.1\n\
             output = _CLK\n\
             [lfo]\n\
             button = B1.2\n\
             input = _CLK\n\
             [env]\n\
             button = B1.3\n\
             rate = 0.5\n");
        let b = p("[p2b8]\n\
             [clock]\n\
             button = B1.1\n\
             output = _CLK\n\
             [lfo]\n\
             button = B1.2\n\
             rate = 0.5\n\
             [env]\n\
             button = B1.3\n\
             input = _CLK\n");
        let r = diff_patches(&a, &b);
        assert!(r.added_cables.is_empty());
        assert!(r.removed_cables.is_empty());
        assert_eq!(r.changed_cables.len(), 1);
        let cc = &r.changed_cables[0];
        assert_eq!(cc.cable, "_CLK");
        // old sinks point to lfo, new sinks point to env
        assert!(cc.old_sinks.iter().any(|(nid, _)| nid.0 == "lfo"));
        assert!(cc.new_sinks.iter().any(|(nid, _)| nid.0 == "env"));
        assert_ne!(cc.old_sinks, cc.new_sinks);
        // sources unchanged -> both singletons clock
        assert_eq!(cc.old_sources, vec!["clock".to_string()]);
        assert_eq!(cc.new_sources, vec!["clock".to_string()]);
    }

    #[test]
    fn cable_source_changed_reported_as_changed_cable() {
        // Same cable, source circuit name differs (clock vs osc)
        let a = p("[p2b8]\n\
             [clock]\n\
             button = B1.1\n\
             output = _CLK\n\
             [lfo]\n\
             button = B1.2\n\
             input = _CLK\n");
        let b = p("[p2b8]\n\
             [osc]\n\
             button = B1.1\n\
             output = _CLK\n\
             [lfo]\n\
             button = B1.2\n\
             input = _CLK\n");
        let r = diff_patches(&a, &b);
        assert_eq!(r.changed_cables.len(), 1);
        let cc = &r.changed_cables[0];
        assert_eq!(cc.cable, "_CLK");
        assert_eq!(cc.old_sources, vec!["clock".to_string()]);
        assert_eq!(cc.new_sources, vec!["osc".to_string()]);
    }

    #[test]
    fn changed_param_value_detected() {
        let a = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\ndecay = 0.2\n");
        let b = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.9\ndecay = 0.2\n");
        let r = diff_patches(&a, &b);
        assert!(r.added_nodes.is_empty() && r.removed_nodes.is_empty());
        assert_eq!(r.changed_nodes.len(), 1);
        let cn = &r.changed_nodes[0];
        assert_eq!(cn.id, ("lfo".to_string(), 0));
        assert_eq!(cn.changed_params, vec!["rate".to_string()]);
    }

    #[test]
    fn changed_param_multiple_keys() {
        let a = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\ndecay = 0.2\nshape = sine\n");
        let b = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.9\ndecay = 0.8\nshape = sine\n");
        let r = diff_patches(&a, &b);
        assert_eq!(r.changed_nodes.len(), 1);
        assert_eq!(
            r.changed_nodes[0].changed_params,
            vec!["decay".to_string(), "rate".to_string()]
        );
    }

    #[test]
    fn param_reorder_equal() {
        let a = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\ndecay = 0.2\nshape = sine\n");
        let b = p("[p2b8]\n[lfo]\nbutton = B1.1\nshape = sine\nrate = 0.5\ndecay = 0.2\n");
        let r = diff_patches(&a, &b);
        assert!(r.added_nodes.is_empty());
        assert!(r.removed_nodes.is_empty());
        assert!(
            r.changed_nodes.is_empty(),
            "param reorder must not be a diff, got {:?}",
            r.changed_nodes
        );
        assert!(
            r.added_cables.is_empty() && r.removed_cables.is_empty() && r.changed_cables.is_empty()
        );
    }

    #[test]
    fn cable_params_excluded_from_changed_nodes() {
        // Changing a cable value (input = _A -> input = _B) must not appear as changed_nodes;
        // it surfaces as cable add/remove instead. The shared lfo node has the same non-cable
        // param, so changed_nodes stays empty even though cable wiring differs.
        let a = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _CLK\n\
             [lfo]\nbutton = B1.2\ninput = _CLK\nrate = 0.5\n");
        let b = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _OTHER\n\
             [lfo]\nbutton = B1.2\ninput = _OTHER\nrate = 0.5\n");
        let r = diff_patches(&a, &b);
        assert!(
            r.changed_nodes.is_empty(),
            "cable values must be excluded from node diff, got {:?}",
            r.changed_nodes
        );
        // cables differ instead
        assert!(r.added_cables.contains(&"_OTHER".to_string()));
        assert!(r.removed_cables.contains(&"_CLK".to_string()));
    }

    #[test]
    fn deterministic_same_result_on_repeated_calls() {
        let a = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _CLK\n\
             [lfo]\nbutton = B1.2\ninput = _CLK\nrate = 0.5\n\
             [env]\nbutton = B1.3\ndecay = 0.2\n");
        let b = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _CLK\n\
             [lfo]\nbutton = B1.2\ninput = _CLK\nrate = 0.9\n\
             [osc]\nbutton = B1.4\ninput = _CLK\n");
        let r1 = diff_patches(&a, &b);
        let r2 = diff_patches(&a, &b);
        assert_eq!(r1, r2, "diff must be deterministic");
    }

    #[test]
    fn deterministic_outputs_sorted() {
        // Construct patches where added/removed/changed ordering would be
        // non-deterministic if not sorted: multiple cables/nodes out of alpha order
        let a = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _ZZZ\n\
             [lfo]\nbutton = B1.2\ninput = _ZZZ\nrate = 0.1\n\
             [env]\nbutton = B1.3\nrate = 0.1\n");
        let b = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _AAA\n\
             [env]\nbutton = B1.3\nrate = 0.9\n\
             [lfo]\nbutton = B1.2\ninput = _AAA\nrate = 0.1\n\
             [osc]\nbutton = B1.4\nrate = 0.2\n");
        let r = diff_patches(&a, &b);
        // added/removed are sorted
        let mut sorted_added = r.added_cables.clone();
        sorted_added.sort();
        assert_eq!(r.added_cables, sorted_added, "added_cables must be sorted");
        let mut sorted_removed = r.removed_cables.clone();
        sorted_removed.sort();
        assert_eq!(
            r.removed_cables, sorted_removed,
            "removed_cables must be sorted"
        );
        let mut sorted_added_nodes = r.added_nodes.clone();
        sorted_added_nodes.sort();
        assert_eq!(
            r.added_nodes, sorted_added_nodes,
            "added_nodes must be sorted"
        );
        let mut sorted_removed_nodes = r.removed_nodes.clone();
        sorted_removed_nodes.sort();
        assert_eq!(
            r.removed_nodes, sorted_removed_nodes,
            "removed_nodes must be sorted"
        );
        // changed vecs sorted as well
        let mut sorted_changed_cables = r.changed_cables.clone();
        sorted_changed_cables.sort();
        assert_eq!(r.changed_cables, sorted_changed_cables);
        let mut sorted_changed_nodes = r.changed_nodes.clone();
        sorted_changed_nodes.sort();
        assert_eq!(r.changed_nodes, sorted_changed_nodes);
        // per-entry fields sorted
        for cc in &r.changed_cables {
            let mut s = cc.old_sources.clone();
            s.sort();
            assert_eq!(cc.old_sources, s);
            let mut s2 = cc.new_sources.clone();
            s2.sort();
            assert_eq!(cc.new_sources, s2);
        }
        for cn in &r.changed_nodes {
            let mut s = cn.changed_params.clone();
            s.sort();
            assert_eq!(cn.changed_params, s);
        }
    }

    #[test]
    fn empty_diff_when_patches_identical() {
        let ini = "[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n[env]\nbutton = B1.2\ndecay = 0.2\n";
        let a = p(ini);
        let b = p(ini);
        let r = diff_patches(&a, &b);
        assert_eq!(r, DiffReport::default());
    }

    // ------------------------------------------------------------------
    // scope_report tests
    // ------------------------------------------------------------------

    #[test]
    fn scope_report_filters_by_token_influence() {
        // Base patch: two independent chains
        // B1.1 -> _CLK -> lfo,  B1.2 -> _OTHER -> env
        let a = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _CLK\n\
             [lfo]\nbutton = I1.1\ninput = _CLK\nrate = 0.5\n\
             [clk2]\nbutton = B1.2\noutput = _OTHER\n\
             [env]\nbutton = I1.2\ninput = _OTHER\ndecay = 0.2\n");
        // B adds a new cable _EXTRA on the B1.1 chain and changes lfo rate,
        // plus adds a new cable _EXTRA2 on the B1.2 chain
        let b = p("[p2b8]\n\
             [clk]\nbutton = B1.1\noutput = _CLK\n\
             [lfo]\nbutton = I1.1\ninput = _CLK\nrate = 0.9\n\
             [extra]\nbutton = I1.3\ninput = _CLK\n\
             [clk2]\nbutton = B1.2\noutput = _OTHER\n\
             [env]\nbutton = I1.2\ninput = _OTHER\ndecay = 0.8\n");
        let full = diff_patches(&a, &b);
        // Full has changed_nodes for lfo and env (and maybe more)
        assert!(full.changed_nodes.len() >= 2);
        // Scope to B1.1 should keep only lfo-related diff, not env
        let scoped = scope_report(&full, "B1.1", &b);
        // lfo = ("lfo",0) should be present, env should not
        assert!(
            scoped
                .changed_nodes
                .iter()
                .any(|cn| cn.id == ("lfo".to_string(), 0)),
            "B1.1 scope must include lfo, got {:?}",
            scoped.changed_nodes
        );
        assert!(
            !scoped
                .changed_nodes
                .iter()
                .any(|cn| cn.id == ("env".to_string(), 0)),
            "B1.1 scope must NOT include env, got {:?}",
            scoped.changed_nodes
        );
        assert!(scoped.changed_nodes.len() < full.changed_nodes.len());
        // Scoped is subset of full
        for nid in &scoped.added_nodes {
            assert!(full.added_nodes.contains(nid));
        }
        for c in &scoped.added_cables {
            assert!(full.added_cables.contains(c));
        }
    }

    #[test]
    fn scope_report_unknown_token_returns_empty() {
        let a = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.5\n");
        let b = p("[p2b8]\n[lfo]\nbutton = B1.1\nrate = 0.9\n[env]\nbutton = B1.2\ndecay = 0.2\n");
        let full = diff_patches(&a, &b);
        assert!(!full.changed_nodes.is_empty() || !full.added_nodes.is_empty());
        let scoped = scope_report(&full, "B9.9", &b);
        assert_eq!(
            scoped,
            DiffReport::default(),
            "unknown token must yield empty report"
        );
    }

    #[test]
    fn scope_report_subset_and_sorted() {
        let a = p("[p2b8]\n[clk]\nbutton = B1.1\noutput = _CLK\n[lfo]\ninput = _CLK\nrate = 0.1\n");
        let b = p("[p2b8]\n[clk]\nbutton = B1.1\noutput = _CLK\n[lfo]\ninput = _CLK\nrate = 0.9\n[env]\nbutton = B1.2\ndecay = 0.2\n");
        let full = diff_patches(&a, &b);
        let scoped = scope_report(&full, "B1.1", &b);
        // subset check
        for c in &scoped.added_cables {
            assert!(full.added_cables.contains(c));
        }
        for c in &scoped.removed_cables {
            assert!(full.removed_cables.contains(c));
        }
        // sorted
        let mut s = scoped.added_cables.clone();
        s.sort();
        assert_eq!(scoped.added_cables, s);
        let mut s2 = scoped.added_nodes.clone();
        s2.sort();
        assert_eq!(scoped.added_nodes, s2);
    }

    #[test]
    fn scope_report_deterministic() {
        let a = p("[p2b8]\n[clk]\nbutton = B1.1\noutput = _CLK\n[lfo]\ninput = _CLK\nrate = 0.5\n");
        let b = p("[p2b8]\n[clk]\nbutton = B1.1\noutput = _CLK\n[lfo]\ninput = _CLK\nrate = 0.9\n");
        let full = diff_patches(&a, &b);
        let s1 = scope_report(&full, "B1.1", &b);
        let s2 = scope_report(&full, "B1.1", &b);
        assert_eq!(s1, s2);
    }
}
