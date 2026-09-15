## Context

See proposal.md (Why). Current state: `App::recompute_influence` writes `graph.highlighted_nodes/edges` from `InfluenceSubtree` and the renderer dims the rest (FULL-graph highlight). `App::apply_dependency_subset` already builds an induced subset `Graph` (nodes/edges cloned by index, empty clusters, subset-relative pins, `layout::solve`, positions mapped back into the full-length array). The renderer draws only `dependency_nodes/edges` when the filter is active (`gui/graph.rs` visibility closures).

## Goals / Non-Goals

- Goals: subset builder with inherited clusters, own layout solve, separate camera fit, toggle wiring, rebuild integration.
- Non-goals: see proposal.md. No shared-subset machinery between dependency and influence filters; deliberate duplication of the small subset pattern over premature abstraction.

## Decisions

- **Mirror `apply_dependency_subset`, don't generalize it.** The dependency path drops clusters (`clusters: Vec::new()`); the influence path must inherit them. One shared function with a keep-clusters flag would couple two independent filters; two small functions keep each filter's contract readable. Chosen: new `apply_influence_subset` next to the existing one.
- **Edge membership by endpoint, not by cable name.** `influenced_edges` holds cable names, but an edge qualifies when both endpoints are in `influenced_nodes`. Rationale: register edges share cables outside the walk; endpoint containment is the induced-subgraph definition and can't leak uninfluenced nodes.
- **Clusters inherited by section-range containment.** A cluster whose `section_range` intersects the subset's sections is kept with its original range. Rationale: zero remapping, renderer draws containers only around visible members (same as today).
- **Separate camera fit via existing `fit_to_world`.** The subset gets its own fit on toggle; focus/zoom state stays per-view so toggling back restores the full view. Rationale: reuses the tested fit path, no new camera math.
- **Toggle lives next to the dependency filter key family on the graph surface.** Reuses focus/hover conventions (`hovered_graph_node`, selected circuit). Exact key chosen at implementation to avoid colliding with `f`/`x`/`p`.

## Risks / Trade-offs

- [Risk] Two subset code paths drift apart → Mitigation: unit tests pin both builders' contracts (membership, pins, determinism).
- [Risk] Subset solve surprises on tiny graphs (1-2 nodes) → Mitigation: solver already handles small graphs; add a 2-node test.
- [Risk] Rebuild ordering (tension/select cycles) drops the influence subset → Mitigation: re-apply subset in `rebuild_graph` next to the dependency re-apply, with the same root-vanished clearing rule.

## Migration Plan

Not applicable (local feature branch, no data migration). Rollback: revert the branch.

## Open Questions

None. All unknowns are implementation detail covered by the task list.
