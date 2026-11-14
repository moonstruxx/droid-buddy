# Proposal

## Why

The signal-flow graph's banner-group titles — the only text that says what a group of circuits is — are the least legible text in the app, in the default palette. Measured with the project's own WCAG formula, `graph_cluster_title` in `classic` is ANSI Blue on the graph canvas' black background: **2.44:1**, below even WCAG's 3:1 non-text bar and far below the 4.5:1 text bar. `ARCHITECTURE.md` §15 records the symptom from the sections visual review ("titles render at very low contrast, so a group's name is hard to read until the user zooms in. Not fixed here.").

There is a second half to the same symptom that contrast alone does not explain: the painter sizes a title as `CLUSTER_TITLE_PX × zoom` and omits it entirely below 7 px, i.e. below zoom 0.58. A large patch opens fitted at a much lower zoom than that, so its banner names are not dim at fit — they are **absent**, and the enclosing rectangles are anonymous. Node titles already take the opposite approach (fixed 12 pt screen-space text, ADR 43 / `graph-node-scaling` "Level of detail"), which is why node names stay readable when zoomed out and group names do not.

## What Changes

- **Classic palette:** `graph_cluster_title` moves to a legible blue (`Color::LightBlue`, 6.45:1 on the canvas background) instead of ANSI Blue (2.44:1). Mono already passes at 5.32:1 and stays untouched; the terminal palette resets every token and is exempt.
- **Paint:** cluster titles render at a fixed screen-space size, like node labels, instead of shrinking with the camera — and are omitted only when the enclosing box has no room for the text in screen space, instead of whenever the zoom-derived size falls below a floor.
- **Tests:** a palette-level contrast guard holds each shipped palette's cluster title to a 4.5:1 text floor against that palette's graph canvas background (unresolvable `Reset` pairs exempt), and a paint-level guard proves the title font size is independent of zoom and that a small box still omits its title.
- The `theming` capability gains the contrast requirement; `graph-node-scaling`'s level-of-detail requirement now states the cluster-title rule its scenario previously delegated to "their own legibility threshold".

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `theming`: a new requirement — every shipped palette's cluster-title token must meet a text-contrast floor against that palette's graph canvas background, with the `classic` token value named.
- `graph-node-scaling`: the "Level of detail at extreme zoom" requirement changes — cluster titles become fixed screen-space text gated on box room, replacing the zoom-derived size with a 7 px omission floor.

## Impact

- `src/theme.rs`: the `classic` palette's `graph_cluster_title` value plus a palette-level contrast test; reuses `rendermetrics::{rgb_luminance, contrast_ratio, color_to_rgb}` (widened to `pub(crate)`) rather than duplicating the WCAG formula.
- `src/gui/graph.rs`: the cluster block in `paint_scene` (`CLUSTER_TITLE_PX`, `MIN_LABEL_PX`) and its tests.
- `openspec/specs/theming/spec.md`, `openspec/specs/graph-node-scaling/spec.md` (via this change's deltas).
- `docs/views.md` (one sentence on the graph's cluster containers) and the regenerated `ARCHITECTURE.md` (§15's open bullet closes; the footer records the change).
- No behavior change outside the graph pane's cluster titles: node labels, edges, minimap, fit camera, sections mode (which draws no cluster boxes), and the other two palettes' tokens are untouched.

## Non-goals

- The cluster **border** stays ANSI Blue. The defect is the group's name; the border is low-profile container chrome and deliberately does not compete with the nodes it encloses.
- No new theme token: the change picks an existing ANSI-16 variant, so the token list, the palettes' structural contract, and `config.toml` stay as they are.
- No dynamic contrast computation at paint time (no luminance lifting of arbitrary user palettes). The rule is asserted by tests over the shipped palettes; a future user-supplied palette catalog would need its own guard and is out of scope.
- Sections mode gains no cluster rectangles (ADR 48 keeps them out); only column and force modes draw titles, exactly as today.
