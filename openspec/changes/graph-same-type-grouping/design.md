# Design

## Context

See `proposal.md` — Why. Three facts shape the approach.

1. The graph already publishes display positions (`App::graph_display_positions`, settle-aware) and world-space node extents (`layout::node_world_sizes`, ADR 40). A grouping pass needs exactly those two inputs and nothing from the painter.
2. The interaction primitives an isolate needs already exist: `Graph::induced_subgraph` and `App::apply_dependency_subset` solve a subset as its own layout and refit the camera (the dependency filter `f` and influence filter `i` are the same shape).
3. The existing edge-colour precedence (topology error > diff > influence > cable kind) and the renderer-owned geometry pattern (ADR 4 / ADR 41) are the natural hooks for group-level highlight answers.

## Goals / Non-Goals

**Goals**
- A large patch stops being dominated by identical stacked nodes and their aggregate fan-out.
- Same-type units stay discoverable: peek, expand, isolate.
- Nothing about the layout becomes non-deterministic and nothing is re-solved to group.

**Non-Goals**
- No new arrangement, no solver tuning, no persisted view state.
- No grouping of controller or jack nodes.
- No new theme token, and no minimap, camera, or settle-animation change.

## Decisions

### D1 — Post-solve view transform, not an arrangement

Grouping runs after `solve_graph_positions` and consumes display positions; a group's drawn position is its members' bounding-box centre. No re-solve and no new `LayoutMode`, so the determinism, pin-anchor and non-overlap contracts (ADRs 18, 29, 48, 49) are untouched, and `Shift+O` makes the change fully reversible at runtime.

*Alternatives considered.* A fourth arrangement would duplicate the solver plumbing and force every existing arrangement to grow a grouping variant; rejected under YAGNI.

### D2 — Clustering by spatial proximity OR at most two cable hops

Union-find over same-type circuit nodes, merged when either (a) their drawn extents are within a proximity ratio of each other, or (b) their distance along cables is at most the hop limit (2). Components with at least `min_group_size` members become groups; the rest stay individual. Union-find processes nodes in ascending index and every derived collection is sorted, so the same graph, positions and settings always produce an identical view.

Spatial proximity carries the work because same-type circuits rarely cable to each other: in `melody3` only 38 cables connect same-type circuits, so one hop alone yields 2 groups, two hops yield 17, spatial proximity yields 30, and the union yields 21–26 depending on the hop budget.

### D3 — Cable aggregation by mapping endpoints onto the collapsed view

For every original edge, map both endpoints to their view node (group or individual), drop the edge when both land in the same group, and deduplicate the rest. Measured on `melody3` at the default minimum size: 1290 edges become 474, and **zero** edges are dropped as internal — the reduction is fan-out deduplication, not hidden topology.

### D4 — Interaction reuses the filter machinery and the world-space hit path

`o` builds the induced subgraph of the group's members plus their immediate neighbours, marks the neighbours dimmed, solves it and refits the camera — the same shape as `toggle_dependency_filter`. Group geometry is world-space like a node (ADR 40), so hover and click travel the existing pointer path rather than a new rect list.

### D5 — Group-level highlight answers under the existing precedence

A group takes the strongest class among its members: topology error, then diff classification, then influence wash, then the circuit-kind colour. Selection lights the group that contains `selected_circuit`. `selected_component` (the module-UI element focus) keeps its register-entity behaviour unchanged.

### D6 — Minimal configuration

Only `group_enabled` and `min_group_size` are configurable; the hop limit and the proximity ratio stay module constants, promoted only if a real need appears (YAGNI).

### D7 — Keys verified free on the graph pane

On the graph pane `o` is only the `g o` chord (prefix-gated), `Char('O')` is not bound anywhere, and `Enter` is a no-op because the shared handler's Enter/Space branch requires a hovered component. No existing binding moves.

## Risks / Trade-offs

- **[Risk] Over-collapsing hides detail.** → The minimum group size (default 5) keeps small components individual, and expand and isolate reach any member.
- **[Risk] Spatial-only would miss cabled chains, wire-only misses stacks.** → The union, with spatial primary and a two-hop supplement.
- **[Risk] A group is not draggable or pinnable, so a cluster cannot be hand-placed.** → Expand or isolate to reach individual nodes; acceptable for a view convenience, and the solver's own placement is unchanged.
- **[Risk] Recomputing clustering per frame is wasteful.** → The view is cached on `App` and invalidated on graph rebuild, settings change, expand and isolate; per-frame reads are cheap.
- **[Risk] A group's highlight could mask a single member's error.** → Precedence is applied at group level and a group carrying an error is visually distinct; expanding reveals the exact member.

## Migration Plan

Single binary, no persisted view state, no data migration. An existing `config.toml` without a `[graph]` table gets grouping enabled at the default minimum size; the table is written only when a `[graph]` setting changes. Rollback is the reverse commit; a stale `[graph]` table is ignored by older builds because serde ignores unknown keys.

## Open Questions

None. The hop limit was settled at two hops during review.
