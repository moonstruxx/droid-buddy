# Design

## Context

See proposal.md — Why. Three facts shape the approach.

1. The painter already projects everything to canvas pixels before drawing clusters: `build_scene_spec` stores a cluster's `x`/`y`/`w`/`h` after the camera transform (padded member union, `CLUSTER_PAD × zoom`), so the enclosing box's screen size is available at paint time without a second projection.
2. Node labels already solved this exact problem: `NODE_LABEL_SIZE` is a fixed 12 pt, independent of zoom, ellipsized to the frame and omitted only when the frame cannot hold one character (ADR 43, `graph-node-scaling`). Cluster titles are the leftover that still scales, and the constant `CLUSTER_TITLE_PX` documents the intended parity ("Matches the cluster-title base size").
3. Palette contrast is already computed in this codebase: `rendermetrics::{color_to_rgb, rgb_luminance, contrast_ratio}` implement WCAG 2.x over the same ANSI-16 table `Theme::rgb` uses. The panel-surface metrics use them; the graph tokens were never held to the same bar.

## Goals / Non-Goals

**Goals:**
- A banner-group name is readable on the default palette and at the zoom a large graph opens with.
- One rule for graph text: screen-space constant size, box-room gating — no per-element zoom floors to keep in sync.
- The contrast floor is a test, not a convention, so the next palette edit cannot quietly regress it.

**Non-Goals:**
- No pixel-level styling beyond the two knobs the rule needs (the fixed title size and the minimum room a box must offer).
- No contrast lifting at paint time; the shipped palettes are asserted, not adapted.
- No change to node labels, ports, edges, the fit camera, or sections mode.

## Decisions

### D1 — Fixed screen-space cluster titles, gated on box room (not zoom)

Paint a title at `CLUSTER_TITLE_PX` points regardless of camera zoom, and decide visibility from the box's already-projected screen rect: the box must be tall enough for one title line plus the existing 3 pt top inset, and wide enough for a readable fragment of the title at that size. This replaces `CLUSTER_TITLE_PX × zoom` clamped to `MIN_LABEL_PX` with a `raw ≥ 7 px` gate, which omits every title below zoom 0.58 — the fit zoom of any large patch.

*Alternatives considered.*
- **Keep zoom-scaled text and raise `MIN_LABEL_PX`**: makes titles appear *later* (a higher floor means a higher zoom is needed), i.e. exactly backwards.
- **Draw the title only when it fits its box in world space**: reintroduces zoom dependence and would hide titles on big boxes at low zoom, where there is plenty of screen room.
- **Truncate a title to whatever fits a tiny box**: an ellipsized fragment names nothing (the same argument ADR 52 makes for cell labels), so a box without room is better left anonymous.

`MIN_LABEL_PX` has no other reader — the port-marker threshold is the separate `LOD_MIN_NODE_H` — so it is deleted with the old gate rather than repurposed (the repo's YAGNI rule: an unread constant goes).

### D2 — Classic title moves within the blue family, border untouched

`graph_cluster_title: Color::Blue → Color::LightBlue` (2.44:1 → 6.45:1). LightBlue is an existing ANSI-16 variant used nowhere in the classic palette's graph tokens, so the group keeps its blue identity while clearing AA text contrast without introducing a token. `mono` already passes (Gray, 5.32:1) and `terminal` resets both tokens, so only the default palette changes.

*Alternatives considered.* `White` (21:1) would compete with `graph_node_border: White` and erase the group/host distinction; `Cyan` (16.75:1) is the control-cable edge colour and would read as topology. `Yellow` is the node title.

The border stays ANSI Blue at 2.44:1: it is container chrome, not an operator, and the defect the issue records is the group's *name*. Raising it is a separate judgement about chrome weight and is listed as a non-goal so the choice is explicit rather than accidental.

### D3 — Reuse the WCAG helpers instead of a second implementation

`rendermetrics`'s three helpers become `pub(crate)` and the palette test calls them with `Theme::rgb` for the title and canvas background. Duplicating the sRGB→luminance curve in `theme.rs` would create a second source of truth for the formula the render-metrics artifact already depends on (the repo's DRY rule: one source per fact).

The test enumerates `theme::THEMES`, skips a palette whose pair does not resolve (`Reset`), and fails below 4.5:1 — so a future palette entry that forgets the rule fails the suite rather than shipping dim text.

### D4 — Prove the paint rule by observing the painted font size

The paint test drives `paint_scene` through a plain `egui::Context` (the repo's existing headless pattern) at two camera zooms and asserts the emitted `Shape::Text` font size is identical, that a title appears at a low zoom where the old rule omitted it, and that a deliberately tiny box paints none. Asserting the constant directly would pass even if the painter ignored it; asserting the painted shape is what the user actually sees.

## Risks / Trade-offs

- [Risk] A fixed 12 pt title on a small box could overflow the rectangle at low zoom → Mitigation: the box-room gate measures the *projected* rect, and the width gate reserves room for a readable fragment before drawing; a box that cannot hold one title line draws nothing.
- [Risk] Titles at fit zoom crowd the boxes and overlap the node labels drawn on top of them → Mitigation: z-order is unchanged (clusters paint first, nodes cover them), the title keeps its existing 6/3 pt inset, and the change makes text that the user could otherwise never see; the alternative was no title at all.
- [Risk] Changing a `classic` token collides with the theming spec's "classic preserves today's exact color mapping" history → Mitigation: the delta states the new value as a requirement with its measured ratio, so the palette's contract now says what it is today; the archive updates the main spec.
- [Risk] `MIN_LABEL_PX` keeps its name while losing its cluster meaning → Mitigation: the design and the code comment name it as the port-marker floor; no other code reads it.

## Migration Plan

Single-binary change, no persisted state, no config migration. A user who had `theme = "classic"` gets the legible title on the next launch; no other token moves. Rollback is the reverse commit (palette value + painter block + tests). The palette contrast test is the guard that makes a partial rollback (reverting the token but keeping the test) fail loudly.

## Open Questions

None. The two styling knobs (fixed title size, minimum box room) are settled in the apply phase against the existing constants and pinned by the paint test; neither changes the specs.
