# Proposal

## Why

On large patches the signal-flow graph renders same-type circuit sections as tall contiguous stacks — a reported screenshot shows roughly thirty `E` (encoder) nodes in one column — with a dense cable fan-out beside them, so a single circuit type dominates the canvas and its collective wiring hides the topology. Those stacks are spatial, so they can be collapsed without touching the layout solver.

Measured on `patches_remote/droid_mpfs5melody3.ini` (593 circuit sections, 1290 cables), collapsing same-type spatial components at the default minimum group size of 5 gives **593 → 201 drawn boxes (−66 %) and 1290 → 474 drawn cables (−63 %)**, with **zero** edges dropped as internal to a group — stacked same-type circuits do not feed each other, so the reduction is fan-out aggregation, not topology hiding. In the same patch only 38 cables connect same-type circuits at all, so spatial proximity must carry the grouping and cable distance can only supplement it.

## What Changes

- **New pure module `src/graph_group.rs`.** Circuit nodes of the same circuit type that are spatially clustered in the drawn layout **or** within two cable hops form components; a component with at least `min_group_size` members (default 5) collapses into one group node, and smaller components render individually.
- **Group rendering.** A collapsed group draws as one box labelled `<circuit> ×<count>` in that circuit's colour; its members' incoming and outgoing cables aggregate and deduplicate onto it, and cables internal to the group are not drawn while it is collapsed.
- **Interaction.** Hovering a group peeks at its contents; clicking it or pressing `Enter` expands it in place into its members (with a group outline) and collapses it again; `o` isolates the hovered or selected group so the pane draws its members plus their dimmed immediate neighbours under a refitted camera; `Shift+O` toggles grouping for the whole graph.
- **Group-level highlight answers.** Selection, diff classification, topology errors, the influence wash and the dependency filter each light a group when any member matches, under the existing edge-colour precedence.
- **Configuration.** `[graph]` in `config.toml` carries `group_enabled` (default `true`) and `min_group_size` (default 5, clamped 2..=64).
- **Invariant.** Grouping is a post-solve view transform: the arrangement, the solver, determinism per input, and pin anchors are unchanged, and a group is placed at its members' bounding-box centre.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `signal-flow-graph`: a new requirement, "Same-type node grouping" — the clustering rule, group rendering and label, cable aggregation, hover/expand/isolate/toggle, group-level highlight answers, determinism, and the post-solve view-transform invariant.
- `keybinding`: a new requirement, "Graph grouping keys" — `o` isolates, `Enter` expands or collapses, `Shift+O` toggles grouping, and the help table lists all three.
- `configuration`: a new requirement, "Graph grouping table" — the `[graph]` table's keys, defaults, clamp, and warn-once fallback.

## Impact

- **New:** `src/graph_group.rs`, registered in `src/lib.rs`.
- `src/config.rs` — a `[graph]` table on `Settings`.
- `src/app.rs`, `src/main.rs` — grouping settings, expanded and isolated state, a cached grouped view, and seeding in `seed_app`.
- `src/gui/graph.rs` — the scene built from the grouped view: group boxes, labels, expanded members, aggregated edges, and the hover popover.
- `src/handler.rs` — `o`, `Enter` and `Shift+O` on the graph pane, plus world-space group hover and click.
- `src/help.rs` — graph key-table rows.
- `docs/views.md`, `docs/keys.md`, `docs/config.md`; regenerated `ARCHITECTURE.md` and `DESIGN.md`.
- No new theme tokens: group boxes reuse the circuit colour tokens.

## Non-goals

- No change to the layout solvers or the arrangements (column, force, sections); grouping is **not** a fourth `LayoutMode`.
- No persistence of expanded or isolated state — session-only, like pins today.
- No grouping of controller or jack nodes; only circuit sections group.
- No per-group drag, pin, or state editing — expand or isolate to reach individual members.
- No new theme token, and no change to the minimap, camera math, or settle animation.
- No grouping support in the headless CLI.
