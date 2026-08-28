//! Pure optimizer core (task 1.2): bounded search over section orderings that
//! minimize forward-loop latency (design D3).
//!
//! DROID evaluates circuits serially in file order once per loop, so the
//! section order *is* part of the cost: a sink reading a cable produced by a
//! later circuit gets last loop's value. [`crate::latency::forward_latency`]
//! turns that wrap-around into a per-edge latency and an aggregate summary;
//! this module searches the space of section permutations for orderings with a
//! lower summary, returning up to three candidates (best first) for the
//! optimizer menu (task 2.1) to preview and export.
//!
//! # Candidate strategies
//!
//! - **banner (default)** — reorder only *within* each banner group's section
//!   range (banner boundaries fixed); minimize total latency.
//! - **global min-sum** — reorder the whole file; minimize total latency.
//! - **min-max** — reorder the whole file; minimize the worst per-edge latency.
//!
//! [`OptimizeScope`] selects which strategies run: `Banner` → 1 candidate,
//! `Global` → 2, `MinMax` → 3. Candidates are sorted best-first by the shared
//! min-sum objective (the min-max candidate is included as a worst-case
//! alternative even though it optimizes `max`, not the sum).
//!
//! # Constraints
//!
//! **Same-name relative order is a hard constraint.** DROID's saved-state
//! mapping is keyed by instance number (occurrence order of same-named
//! circuits), so a candidate must keep the relative order of same-name
//! sections. A corollary: under that constraint the `NodeId`s (instance
//! indices) and the edge set are *invariant* — only the positions change — so
//! nodes and edges are derived once and every candidate is scored by
//! re-mapping positions through [`forward_latency`].
//!
//! **Deterministic and bounded**: the search is seeded from node-id hashes
//! (FNV-1a, no RNG), capped at ~2000 local-search steps, and small search
//! spaces (≤ [`ENUM_LIMIT`] valid permutations) are solved exactly by
//! enumeration — which is what the brute-force equivalence tests (N ≤ 8)
//! rely on.
//!
//! Pure module: no terminal, no I/O, no RNG. The patch, cost model and schema
//! arrive as arguments, so identical input yields byte-identical candidates.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;

use crate::graph::{GraphEdge, NodeId};
use crate::latency::{forward_latency, CostModel, LatencySummary};
use crate::patch::{IniSection, Patch};
use crate::schema::{load_schema, Schema};

/// Valid permutations above this count switch from exact enumeration to the
/// bounded local search. `8! = 40320` (the largest small-N case the
/// brute-force equivalence tests cover) stays under the limit, so N ≤ 8 always
/// takes the exact path.
const ENUM_LIMIT: u128 = 50_000;

/// Local-search step budget for the heuristic path (each step = one scored
/// candidate permutation).
const SEARCH_STEPS: usize = 2_000;

/// One candidate reordering of the patch's sections.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateOrdering {
    /// Human-readable strategy label, e.g. `"banner (default)"`.
    pub label: String,
    /// Permutation of section indices (`order[i]` = section at file position
    /// `i`), same length as `patch.sections`. Consumed by the writer
    /// (task 2.1/1.3) to emit the reordered `.ini`.
    pub order: Vec<usize>,
    /// Latency summary of the *original* file order (the same for every
    /// candidate of one patch).
    pub before: LatencySummary,
    /// Latency summary of `order`.
    pub after: LatencySummary,
}

/// Which candidate strategies a [`generate_candidates`] call includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizeScope {
    /// Banner-preserving min-sum only (the default): 1 candidate.
    #[default]
    Banner,
    /// Banner-preserving + whole-file min-sum: up to 2 candidates.
    Global,
    /// All three strategies: banner min-sum, global min-sum, min-max:
    /// up to 3 candidates.
    MinMax,
}

/// Objective a strategy minimizes (the scoring key over `LatencySummary`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Objective {
    /// Total latency, i.e. `avg` (proportional to `Σ L` for a fixed edge
    /// count), tie-broken by back-edge count then worst edge.
    MinSum,
    /// Worst per-edge latency, tie-broken by total then back edges.
    MinMax,
}

/// The three candidate strategies.
#[derive(Debug, Clone, Copy)]
enum Strategy {
    BannerMinSum,
    GlobalMinSum,
    MinMax,
}

impl Strategy {
    fn label(self) -> &'static str {
        match self {
            Strategy::BannerMinSum => "banner (default)",
            Strategy::GlobalMinSum => "global min-sum",
            Strategy::MinMax => "min-max",
        }
    }

    fn objective(self) -> Objective {
        match self {
            Strategy::MinMax => Objective::MinMax,
            _ => Objective::MinSum,
        }
    }

    /// Section-index ranges this strategy may permute, in file order.
    fn domains(self, patch: &Patch) -> Vec<Range<usize>> {
        match self {
            Strategy::BannerMinSum => {
                // Hand-built patches carry no banner groups; fall back to a
                // single whole-file domain (banner scope degenerates to the
                // global scope, which is still a valid candidate).
                if patch.banner_groups.is_empty() {
                    std::iter::once(0..patch.sections.len()).collect()
                } else {
                    patch
                        .banner_groups
                        .iter()
                        .map(|g| g.section_range.clone())
                        .collect()
                }
            }
            Strategy::GlobalMinSum | Strategy::MinMax => {
                std::iter::once(0..patch.sections.len()).collect()
            }
        }
    }
}

impl OptimizeScope {
    fn strategies(self) -> Vec<Strategy> {
        match self {
            OptimizeScope::Banner => vec![Strategy::BannerMinSum],
            OptimizeScope::Global => vec![Strategy::BannerMinSum, Strategy::GlobalMinSum],
            OptimizeScope::MinMax => vec![
                Strategy::BannerMinSum,
                Strategy::GlobalMinSum,
                Strategy::MinMax,
            ],
        }
    }
}

/// Nodes and edges of a patch, mirroring `graph.rs::build_nodes` /
/// `build_edges`. Because candidates preserve same-name relative order, the
/// `NodeId`s and the edge set are identical for every valid permutation — only
/// the processing positions change.
struct Derived {
    /// `(NodeId, section_index)` per section, in file order.
    nodes: Vec<(NodeId, usize)>,
    /// One directed edge per (cable, source, sink), sorted deterministically
    /// (same as `graph.rs::build_edges`).
    edges: Vec<GraphEdge>,
}

/// Everything a candidate evaluation needs: the derived graph plus the cost
/// provider and schema (bundled to keep the search helpers' signatures small).
struct EvalCtx<'a> {
    derived: &'a Derived,
    cost: &'a CostModel,
    schema: &'a Schema,
}

fn derive(patch: &Patch) -> Derived {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let nodes: Vec<(NodeId, usize)> = patch
        .sections
        .iter()
        .enumerate()
        .map(|(section_index, section)| {
            let instance = counts.entry(section.name.as_str()).or_insert(0);
            let id = (section.name.clone(), *instance);
            *instance += 1;
            (id, section_index)
        })
        .collect();
    // Name → first node, exactly like `graph.rs::name_to_first_node`.
    let mut first: HashMap<&str, NodeId> = HashMap::new();
    for (id, _) in &nodes {
        first.entry(id.0.as_str()).or_insert_with(|| id.clone());
    }
    let mut edges = Vec::new();
    for (cable, entry) in &patch.cable_index {
        for source_name in &entry.sources {
            let Some(source) = first.get(source_name.as_str()) else {
                continue;
            };
            for (sink_name, _) in &entry.sink_refs {
                let Some(sink) = first.get(sink_name.as_str()) else {
                    continue;
                };
                edges.push(GraphEdge {
                    cable: cable.clone(),
                    source: source.clone(),
                    sink: sink.clone(),
                });
            }
        }
    }
    edges.sort_by(|a, b| (&a.cable, &a.source, &a.sink).cmp(&(&b.cable, &b.source, &b.sink)));
    Derived { nodes, edges }
}

/// Score a candidate `order` with the exact `forward_latency` metric: nodes
/// keep their identities, only the per-node processing position changes.
fn evaluate_order(order: &[usize], eval: &EvalCtx<'_>) -> LatencySummary {
    let mut position = vec![0usize; order.len()];
    for (pos, &section) in order.iter().enumerate() {
        position[section] = pos;
    }
    let node_positions: Vec<(NodeId, usize)> = eval
        .derived
        .nodes
        .iter()
        .map(|(id, section_index)| (id.clone(), position[*section_index]))
        .collect();
    let circuit_avg = |id: &NodeId| eval.cost.circuit_avg(id, eval.schema);
    let (_, summary) = forward_latency(&eval.derived.edges, &node_positions, circuit_avg);
    summary
}

/// Total order over summaries for a strategy: `Less` = strictly better.
fn cmp_summaries(a: &LatencySummary, b: &LatencySummary, objective: Objective) -> Ordering {
    match objective {
        Objective::MinSum => a
            .avg
            .total_cmp(&b.avg)
            .then_with(|| a.back_edge_count.cmp(&b.back_edge_count))
            .then_with(|| a.max.total_cmp(&b.max)),
        Objective::MinMax => a
            .max
            .total_cmp(&b.max)
            .then_with(|| a.avg.total_cmp(&b.avg))
            .then_with(|| a.back_edge_count.cmp(&b.back_edge_count)),
    }
}

/// `true` when `a` is strictly better than `b` under `objective`.
fn better(a: &LatencySummary, b: &LatencySummary, objective: Objective) -> bool {
    cmp_summaries(a, b, objective) == Ordering::Less
}

/// FNV-1a 64-bit — a tiny deterministic hash for the search seed (no RNG).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Number of orderings of `domain` (as section indices) that preserve
/// same-name relative order: `N! / ∏ cnt_name!`, clamped above `ENUM_LIMIT`
/// (only the `<= ENUM_LIMIT` decision matters, so clamping is safe).
fn valid_permutation_count(domain: &[usize], sections: &[IniSection]) -> u128 {
    let n = domain.len() as u128;
    let mut counts: HashMap<&str, u128> = HashMap::new();
    for &i in domain {
        *counts.entry(sections[i].name.as_str()).or_insert(0) += 1;
    }
    let mut numerator: u128 = 1;
    for k in 2..=n {
        numerator = numerator.saturating_mul(k);
    }
    let mut denominator: u128 = 1;
    for &c in counts.values() {
        for k in 2..=c {
            denominator = denominator.saturating_mul(k);
        }
    }
    let count = numerator.checked_div(denominator).unwrap_or(0);
    count.min(ENUM_LIMIT + 1)
}

/// Deterministic seed: within each domain, sort sections by `(hash(name),
/// instance)` — same-named sections keep their instance order (valid
/// permutation), different names are ordered by hash.
fn seed_order(domains: &[Range<usize>], sections: &[IniSection]) -> Vec<usize> {
    let mut order = Vec::with_capacity(sections.len());
    for range in domains {
        let mut items: Vec<(u64, usize, usize)> = Vec::new();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for (i, section) in sections
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.len())
        {
            let name = section.name.as_str();
            let instance = counts.entry(name).or_insert(0);
            items.push((fnv1a(name.as_bytes()), *instance, i));
            *instance += 1;
        }
        items.sort_unstable();
        order.extend(items.into_iter().map(|(_, _, i)| i));
    }
    order
}

/// `true` when swapping `order[i]` and `order[j]` preserves same-name relative
/// order: the two sections must have different names, and no section strictly
/// between them may share either name (otherwise the swap would invert a
/// same-name pair's relative order).
fn safe_swap(order: &[usize], sections: &[IniSection], i: usize, j: usize) -> bool {
    let (lo, hi) = (i.min(j), i.max(j));
    let name_lo = &sections[order[lo]].name;
    let name_hi = &sections[order[hi]].name;
    if name_lo == name_hi {
        return false;
    }
    for &mid in &order[lo + 1..hi] {
        let name_mid = &sections[mid].name;
        if name_mid == name_lo || name_mid == name_hi {
            return false;
        }
    }
    true
}

/// Enumerate every ordering of `domain` (as section indices) that preserves
/// same-name relative order, in deterministic order (names sorted).
fn enumerate_domain(domain: &[usize], sections: &[IniSection]) -> Vec<Vec<usize>> {
    // name → its occurrence positions in `domain`, in file order.
    let mut occurrences: HashMap<&str, Vec<usize>> = HashMap::new();
    for &i in domain {
        occurrences
            .entry(sections[i].name.as_str())
            .or_default()
            .push(i);
    }
    let mut names: Vec<&str> = occurrences.keys().copied().collect();
    names.sort_unstable();
    let mut out = Vec::new();
    let mut order = Vec::with_capacity(domain.len());
    let mut remaining: HashMap<&str, usize> = names
        .iter()
        .map(|&name| (name, occurrences[name].len()))
        .collect();
    enumerate_rec(&occurrences, &names, &mut remaining, &mut order, &mut out);
    out
}

fn enumerate_rec<'a>(
    occurrences: &HashMap<&'a str, Vec<usize>>,
    names: &[&'a str],
    remaining: &mut HashMap<&'a str, usize>,
    order: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if remaining.values().all(|&left| left == 0) {
        out.push(order.clone());
        return;
    }
    for &name in names {
        let Some(&left) = remaining.get(name) else {
            continue;
        };
        if left == 0 {
            continue;
        }
        // The next unused occurrence of this name is placed next; occurrence
        // lists are in file order, so same-name sections keep relative order.
        let idx = occurrences[name].len() - left;
        order.push(occurrences[name][idx]);
        remaining.insert(name, left - 1);
        enumerate_rec(occurrences, names, remaining, order, out);
        remaining.insert(name, left);
        order.pop();
    }
}

/// Combine per-domain orderings (cartesian product) and keep the best full
/// order under `objective`. Deterministic: ties keep the first order found.
fn combine_rec(
    per_domain: &[Vec<Vec<usize>>],
    d: usize,
    order: &mut Vec<usize>,
    best: &mut Option<(Vec<usize>, LatencySummary)>,
    eval: &EvalCtx<'_>,
    objective: Objective,
) {
    if d == per_domain.len() {
        let summary = evaluate_order(order, eval);
        let take = match best {
            None => true,
            Some((_, best_summary)) => better(&summary, best_summary, objective),
        };
        if take {
            *best = Some((order.clone(), summary));
        }
        return;
    }
    for perm in &per_domain[d] {
        let start = order.len();
        order.extend_from_slice(perm);
        combine_rec(per_domain, d + 1, order, best, eval, objective);
        order.truncate(start);
    }
}

/// Exact search: enumerate all valid full orders (small space only).
fn search_exact(
    domains: &[Range<usize>],
    sections: &[IniSection],
    eval: &EvalCtx<'_>,
    objective: Objective,
) -> (Vec<usize>, LatencySummary) {
    let per_domain: Vec<Vec<Vec<usize>>> = domains
        .iter()
        .map(|d| enumerate_domain(&(d.start..d.end).collect::<Vec<usize>>(), sections))
        .collect();
    let mut best: Option<(Vec<usize>, LatencySummary)> = None;
    let mut order: Vec<usize> = Vec::with_capacity(sections.len());
    combine_rec(&per_domain, 0, &mut order, &mut best, eval, objective);
    best.unwrap_or_else(|| {
        let identity: Vec<usize> = (0..sections.len()).collect();
        let summary = evaluate_order(&identity, eval);
        (identity, summary)
    })
}

/// Heuristic search: seeded from node-id hashes, then first-improvement
/// hill-climbing over safe swaps within each domain, bounded by `SEARCH_STEPS`.
/// Tracks the best order seen; the identity order is always a candidate, so
/// the result is never worse than the original under `objective`.
fn search_local(
    domains: &[Range<usize>],
    sections: &[IniSection],
    eval: &EvalCtx<'_>,
    objective: Objective,
) -> (Vec<usize>, LatencySummary) {
    let mut order = seed_order(domains, sections);
    let mut best_order = order.clone();
    let mut best_summary = evaluate_order(&best_order, eval);
    let identity: Vec<usize> = (0..sections.len()).collect();
    let identity_summary = evaluate_order(&identity, eval);
    if better(&identity_summary, &best_summary, objective) {
        best_order = identity;
        best_summary = identity_summary;
    }
    let mut budget = SEARCH_STEPS;
    loop {
        let mut improved = false;
        // Phase 1: adjacent swaps within each domain (cheap refinement).
        for range in domains {
            if range.end <= range.start + 1 {
                continue;
            }
            for i in range.start..range.end - 1 {
                if budget == 0 {
                    return (best_order, best_summary);
                }
                budget -= 1;
                if sections[order[i]].name == sections[order[i + 1]].name {
                    continue;
                }
                order.swap(i, i + 1);
                let summary = evaluate_order(&order, eval);
                if better(&summary, &best_summary, objective) {
                    best_order = order.clone();
                    best_summary = summary;
                    improved = true;
                } else {
                    order.swap(i, i + 1);
                }
            }
        }
        // Phase 2: arbitrary safe swaps within each domain.
        for range in domains {
            for i in range.start..range.end {
                for j in (i + 1)..range.end {
                    if budget == 0 {
                        return (best_order, best_summary);
                    }
                    budget -= 1;
                    if !safe_swap(&order, sections, i, j) {
                        continue;
                    }
                    order.swap(i, j);
                    let summary = evaluate_order(&order, eval);
                    if better(&summary, &best_summary, objective) {
                        best_order = order.clone();
                        best_summary = summary;
                        improved = true;
                    } else {
                        order.swap(i, j);
                    }
                }
            }
        }
        if !improved {
            return (best_order, best_summary);
        }
    }
}

/// Generate up to three candidate section orderings, best first by the shared
/// min-sum objective. See the module docs for the strategies and constraints.
pub fn generate_candidates(
    patch: &Patch,
    cost: &CostModel,
    scope: OptimizeScope,
) -> Vec<CandidateOrdering> {
    if patch.sections.is_empty() {
        return Vec::new();
    }
    let derived = derive(patch);
    let schema = load_schema();
    let eval = EvalCtx {
        derived: &derived,
        cost,
        schema: &schema,
    };
    if derived.edges.is_empty() {
        // Nothing to optimize: no cables, so every ordering ties. Return the
        // single identity candidate so the caller still gets a valid result.
        let identity: Vec<usize> = (0..patch.sections.len()).collect();
        let summary = evaluate_order(&identity, &eval);
        return vec![CandidateOrdering {
            label: String::from(Strategy::BannerMinSum.label()),
            order: identity,
            before: summary,
            after: summary,
        }];
    }
    let identity: Vec<usize> = (0..patch.sections.len()).collect();
    let before = evaluate_order(&identity, &eval);

    let mut candidates = Vec::new();
    for strategy in scope.strategies() {
        let domains = strategy.domains(patch);
        let total_valid: u128 = domains
            .iter()
            .map(|d| {
                valid_permutation_count(&(d.start..d.end).collect::<Vec<usize>>(), &patch.sections)
            })
            .fold(1u128, |acc, c| acc.saturating_mul(c.min(ENUM_LIMIT + 1)));
        let (order, after) = if total_valid <= ENUM_LIMIT {
            search_exact(&domains, &patch.sections, &eval, strategy.objective())
        } else {
            search_local(&domains, &patch.sections, &eval, strategy.objective())
        };
        candidates.push(CandidateOrdering {
            label: String::from(strategy.label()),
            order,
            before,
            after,
        });
    }
    // Best first by the shared min-sum objective; ties keep strategy order.
    candidates.sort_by(|a, b| cmp_summaries(&a.after, &b.after, Objective::MinSum));
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(content: &str) -> Patch {
        Patch::from_ini_str(content, String::from("optimize-test")).expect("fixture parses")
    }

    /// File order: d, b, a, c. Cables a→c, c→b, b→d (a chain).
    /// Identity latency: avg 7/3, max 3, 2 back edges. The unique optimum is
    /// the chain order `[a, c, b, d]` = section indices `[2, 3, 0, 1]`:
    /// avg 1.0, max 1.0, 0 back edges.
    fn chain_patch() -> Patch {
        patch(
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
        )
    }

    /// 12 distinct sections, file order scrambled, wired as one long chain
    /// s0→s1→…→s11. Search space is 12! ≫ ENUM_LIMIT → exercises the local
    /// search path.
    fn big_patch() -> Patch {
        let mut content = String::from("[s5]\nbutton = B1.1\nin = _C4\noutput = _C5\n");
        for k in 6..=11 {
            content.push_str(&format!("[s{k}]\nin = _C{}\noutput = _C{k}\n", k - 1));
        }
        content.push_str("[s0]\nin = _S\noutput = _C0\n");
        for k in 1..=4 {
            content.push_str(&format!("[s{k}]\nin = _C{}\noutput = _C{k}\n", k - 1));
        }
        patch(&content)
    }

    fn assert_same_name_order(order: &[usize], patch: &Patch) {
        for name in [
            "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11",
        ] {
            let idxs: Vec<usize> = (0..order.len())
                .filter(|&i| patch.sections[order[i]].name == name)
                .collect();
            let sections: Vec<usize> = idxs.iter().map(|&i| order[i]).collect();
            assert!(
                sections.windows(2).all(|w| w[0] < w[1]),
                "same-name sections out of order for {name}: {sections:?}"
            );
        }
    }

    #[test]
    fn candidate_strategy_counts_match_scope() {
        let patch = chain_patch();
        let cost = CostModel::default();
        assert_eq!(
            generate_candidates(&patch, &cost, OptimizeScope::Banner).len(),
            1
        );
        assert_eq!(
            generate_candidates(&patch, &cost, OptimizeScope::Global).len(),
            2
        );
        assert_eq!(
            generate_candidates(&patch, &cost, OptimizeScope::MinMax).len(),
            3
        );
    }

    #[test]
    fn best_candidate_matches_brute_force_for_small_n() {
        let patch = chain_patch();
        let cost = CostModel::default();
        let candidates = generate_candidates(&patch, &cost, OptimizeScope::Global);
        let derived = derive(&patch);
        let schema = load_schema();
        let eval = EvalCtx {
            derived: &derived,
            cost: &cost,
            schema: &schema,
        };

        // Independent brute force: enumerate all 4! permutations of the four
        // sections (names distinct → all valid), score, keep the best.
        let mut best: Option<(Vec<usize>, LatencySummary)> = None;
        let mut perm = vec![0usize, 1, 2, 3];
        let mut stack = [0usize; 4];
        let mut i = 0usize;
        loop {
            let summary = evaluate_order(&perm, &eval);
            let take = match &best {
                None => true,
                Some((_, bs)) => better(&summary, bs, Objective::MinSum),
            };
            if take {
                best = Some((perm.clone(), summary));
            }
            if i >= 4 {
                break;
            }
            if stack[i] < i {
                if i.is_multiple_of(2) {
                    perm.swap(0, i);
                } else {
                    perm.swap(stack[i], i);
                }
                stack[i] += 1;
                i = 0;
            } else {
                stack[i] = 0;
                i += 1;
            }
        }
        let (brute_order, brute_summary) = best.expect("brute force found a best");
        assert_eq!(candidates[0].order, brute_order);
        assert_eq!(candidates[0].after, brute_summary);
        assert_eq!(candidates[0].after.avg, 1.0);
        assert_eq!(candidates[0].after.max, 1.0);
        assert_eq!(candidates[0].after.back_edge_count, 0);
    }

    #[test]
    fn candidates_never_worse_than_original() {
        for (patch, scopes) in [
            (
                chain_patch(),
                vec![
                    OptimizeScope::Banner,
                    OptimizeScope::Global,
                    OptimizeScope::MinMax,
                ],
            ),
            (
                big_patch(),
                vec![OptimizeScope::Global, OptimizeScope::MinMax],
            ),
        ] {
            let cost = CostModel::default();
            for scope in scopes {
                for candidate in generate_candidates(&patch, &cost, scope) {
                    assert!(
                        !better(&candidate.before, &candidate.after, Objective::MinSum),
                        "{} produced a candidate worse than the original",
                        candidate.label
                    );
                }
            }
        }
    }

    #[test]
    fn candidates_are_deterministic() {
        for patch in [chain_patch(), big_patch()] {
            let cost = CostModel::default();
            let first = generate_candidates(&patch, &cost, OptimizeScope::MinMax);
            let second = generate_candidates(&patch, &cost, OptimizeScope::MinMax);
            assert_eq!(first.len(), second.len());
            for (a, b) in first.iter().zip(second.iter()) {
                assert_eq!(a.label, b.label);
                assert_eq!(a.order, b.order);
                assert_eq!(a.before, b.before);
                assert_eq!(a.after, b.after);
            }
        }
    }

    #[test]
    fn same_name_relative_order_preserved() {
        let cost = CostModel::default();
        for patch in [chain_patch(), big_patch()] {
            for candidate in generate_candidates(&patch, &cost, OptimizeScope::MinMax) {
                assert_same_name_order(&candidate.order, &patch);
            }
        }
    }

    #[test]
    fn banner_scope_respects_banner_boundaries() {
        let patch = patch(
            "# ---- Alpha ----\n\
             [a]\n\
             button = B1.1\n\
             output = _AC\n\
             [c]\n\
             in = _AC\n\
             # ---- Beta ----\n\
             [b]\n\
             output = _BD\n\
             [d]\n\
             in = _BD\n",
        );
        let candidates = generate_candidates(&patch, &CostModel::default(), OptimizeScope::Banner);
        assert_eq!(candidates.len(), 1);
        let order = &candidates[0].order;
        let mut group_a: Vec<usize> = (0..2).map(|i| order[i]).collect();
        let mut group_b: Vec<usize> = (2..4).map(|i| order[i]).collect();
        group_a.sort_unstable();
        group_b.sort_unstable();
        assert_eq!(
            group_a,
            vec![0, 1],
            "Alpha sections must stay in their range"
        );
        assert_eq!(
            group_b,
            vec![2, 3],
            "Beta sections must stay in their range"
        );
    }

    #[test]
    fn minmax_bounds_each_edge_by_worst_original() {
        let patch = chain_patch();
        let cost = CostModel::default();
        let candidates = generate_candidates(&patch, &cost, OptimizeScope::MinMax);
        let minmax = candidates
            .iter()
            .find(|c| c.label == "min-max")
            .expect("min-max candidate present");
        assert!(minmax.after.max <= minmax.before.max);

        // Recompute per-edge latencies for the candidate order: none may be
        // worse than the worst original edge.
        let derived = derive(&patch);
        let schema = load_schema();
        let mut position = vec![0usize; minmax.order.len()];
        for (pos, &section) in minmax.order.iter().enumerate() {
            position[section] = pos;
        }
        let node_positions: Vec<(NodeId, usize)> = derived
            .nodes
            .iter()
            .map(|(id, section_index)| (id.clone(), position[*section_index]))
            .collect();
        let circuit_avg = |id: &NodeId| cost.circuit_avg(id, &schema);
        let (edge_latencies, _) = forward_latency(&derived.edges, &node_positions, circuit_avg);
        for edge in &edge_latencies {
            assert!(
                edge.latency <= minmax.before.max,
                "edge latency {} exceeds worst original {}",
                edge.latency,
                minmax.before.max
            );
        }
        assert_eq!(
            minmax.after.max, 1.0,
            "chain order should drop max from 3.0 to 1.0"
        );
    }

    #[test]
    fn empty_and_single_section_graceful() {
        let cost = CostModel::default();
        let mut empty = patch("[a]\nbutton = B1.1\nx = 1\n");
        empty.sections.clear();
        assert!(
            generate_candidates(&empty, &cost, OptimizeScope::MinMax).is_empty(),
            "empty patch → no candidates"
        );

        let single = patch("[a]\nbutton = B1.1\nx = 1\n");
        let candidates = generate_candidates(&single, &cost, OptimizeScope::MinMax);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].order, vec![0]);
        assert_eq!(candidates[0].before, candidates[0].after);
    }
}
