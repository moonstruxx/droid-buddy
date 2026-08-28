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
