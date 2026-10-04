# Design

## Context

See proposal.md (Why). Current state: `physical_spec`/`paint_physical`
(`src/gui/physical.rs`) render each cell's label inside its own rect at fixed
`CELL_LABEL_SIZE`, ellipsized by `clip_label`; resolution is
store → guess → derived via `Patch::display_label` plus
`App.guessed_labels`. `p` today is global processing pause (pin/unpin on the
graph pane). `Esc` on the Physical pane clears shift/modifier before closing.

## Goals / Non-Goals

**Goals:**

- Big readable labels for every rack element without breaking 1:1 geometry.
- Guessed labels as first-class content (hardware has no dynamic labels).
- Zero regression to `p` outside the Physical pane.

**Non-Goals:**

- Full-band takeover; new label sources; solver/pin/pause changes.

## Decisions

- **View-scoped `p`** (over global rebind): the Physical pane owns `p` for
  the performance view; pause stays reachable from every other pane (it is
  global) and pin stays on the graph. Rationale: smallest blast radius, no
  function lost. `Esc` exits, mirroring shift/modifier clearing.
- **Pane-local presentation** (over band takeover): the performance view
  lives inside the Physical pane via the existing spec/painter split.
  Rationale: composes with the class layout, no pane surgery.
- **Side-bound greedy placement** (over exact optimization): assign each
  label a side by host quadrant relative to the rack center, order within a
  side by host coordinate, stack to de-collide. Rationale: label placement
  is NP-hard in general; greedy + stable binding is legible and testable.
  Pure module (`src/performance.rs`), no egui dependency.
- **Guessed-first resolution** (over raw tokens): callouts resolve
  store → guess → derived, reusing `display_label`/`guessed_labels`.
  Rationale: DROID has no dynamic labels, so guessing is the feature.
- **Leader lines in one new theme token** (over hardcoded color): e.g.
  `performance_leader` across classic/terminal/mono. Rationale: the
  token layer forbids raw RGB in paint code.
- **Reset restores rest state only** (over full selection clear): element
  `ComponentState`s return to rest; selection/focus/shift are untouched.
  Rationale: reset is about performance state, not navigation.

## Risks / Trade-offs

- [Risk] 100+ labels on melody2-scale patches crowd the field →
  Mitigation: side stacking degrades to ellipsis + status-bar fallback;
  scoped (focus-neighborhood) filtering is a follow-up, not this change.
- [Risk] `p` muscle memory for pause on the Physical pane →
  Mitigation: status names the view on entry; pause remains one Tab away.
- [Risk] Leader-line crossings on dense rows →
  Mitigation: within-side ordering by host coordinate minimizes crossings;
  dim non-focused lines when a focus exists.
