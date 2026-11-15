# Proposal

## Why

On large patches the signal-flow graph renders same-type circuit sections as tall contiguous stacks — a reported screenshot shows roughly thirty `E` (encoder) nodes in one column — with a dense cable fan-out beside them, so a single circuit type dominates the canvas and its collective wiring hides the topology. Those stacks are spatial, so they can be collapsed without touching the layout solver.

Measured on the user's real patches with grouping by circuit type at the default minimum size of 5: `droid_mpfs5melody3.ini` (686 nodes, 2230 cables) draws **148 bodies and 353 cables instead of 686 and 2230** (−78 % bodies, −84 % cables), and `_droid_mpfs5drum.ini` (634/2340) draws **231/396** (−64 %, −83 %). Cables collapse far more than bodies because the fan-out of same-type units deduplicates onto one group edge. Grouping only by spatial run is much weaker — 615 bodies on melody3 — because one circuit of another type between two encoders breaks the run and splits 54 encoders into boxes of 12/7/6/6/6. So the type policy is the default and the run policy stays selectable for operators who want positional meaning kept.

## What Changes

- **New pure module `src/graph_group.rs`.** Circuit nodes are grouped by circuit type by default: every circuit of one type with at least `min_group_size` drawn members (default 5) collapses into one group node. A run policy is also selectable, which groups only same-type nodes that are spatially clustered **or** within two cable hops. Populations below the minimum render individually.
- **Group rendering.** A collapsed group draws as one box labelled `<circuit> ×<count>` in that circuit's colour; its members' incoming and outgoing cables aggregate and deduplicate onto it, and cables internal to the group are not drawn while it is collapsed.
- **Interaction.** Hovering a group peeks at its contents; clicking it or pressing `Enter` expands it in place into its members (with a group outline) and collapses it again; `o` isolates the hovered or selected group so the pane draws its members plus their dimmed immediate neighbours under a refitted camera; `Shift+O` toggles grouping for the whole graph.
- **Group-level highlight answers.** Selection, diff classification, topology errors, the influence wash and the dependency filter each light a group when any member matches, under the existing edge-colour precedence.
- **Configuration.** `[graph]` in `config.toml` carries `group_enabled` (default `true`), `min_group_size` (default 5, clamped 2..=64) and `group_by` (default `"type"`, or `"run"`).
- **Invariant.** Grouping is a post-solve view transform: the arrangement, the solver, determinism per input, and pin anchors are unchanged, and a group is placed at its members' centre. A group whose members span more than a small multiple of one node height draws as one nominal node-sized box at their centroid, so a spread type does not cover the canvas; a real stack is enclosed.

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
