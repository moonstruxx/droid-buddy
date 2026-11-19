# Proposal

## Why

Two dead ends in the main band. Maximize has exactly one key (`z`) while `f` — the
key at the same reach, and the one an operator presses for "full screen" — is spent
on two pane-scoped meanings: the module UI's labels fit and the graph's dependency
filter. The band's default arrangement is three tiles, and getting one view onto the
whole band should not require knowing a key that is documented one scroll down in a
second table.

And the performance view, whose whole reason to exist is that the labels are the
headline content, draws them at a fixed 14 pt and ignores `physical_zoom` entirely
(`performance_mapping` centres a compact rack at `fit × 0.5` and the zoom keys change
a number nothing reads). On a large pane the rack stays a quarter of the area and the
labels stay 14 pt forever: the view cannot be zoomed in, and the one knob that would
make its content readable does not exist. The 14 pt constant came from
`label-guess-and-screen-scale`, whose decision was "independent of zoom" so labels
survive *being* zoomed — not a rule against letting the operator choose a size.

## What Changes

- **`f` maximizes from any view.** A second binding for the existing non-latching
  maximize, checked in the layout table beside `z`, so it works whatever pane and
  view holds focus, with `z`'s exact clearing rules (`f` again, `Tab`, `Esc`).
- **The keys `f` displaces get new homes, all pane-scoped or prefixed:**
  - module UI labels fit: `f` → `Shift+F`
  - module UI rack fit: `Shift+F` → `g f` (the `g` prefix)
  - graph dependency filter: `f` → `Shift+F`
- **The performance view zooms.** Its mapping multiplies the compact base by
  `physical_zoom` and clamps to the whole-rack fit, so `+`/`-` (and the wheel) scale
  the rack from a quarter of the pane up to the whole pane and stop there — no pan is
  needed because the rack can never overflow.
- **Performance labels scale with that zoom by default**, and their base size is an
  independent knob: `Shift`+`+`/`-` step it by 5 %, persisted as
  `[labels] performance_scale`. At zoom 100 % and scale 100 % the picture is exactly
  today's.
- **One size, two consumers.** The effective callout size rides `CalloutSpec` so the
  placement budget (`callout_size`, `place_labels`) and the painter read the same
  number; label annotations and the hover reveal keep their fixed 14 pt.
- **Documentation**: help tables, `docs/keys.md`, `docs/views.md`, `docs/config.md`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `keybinding`: **MODIFIED** "Physical-view navigation keys" — the labels fit moves to
  `Shift+F` and the rack fit to `g f`. **MODIFIED** "Pane layout keys" — `z` and `f`
  are both the maximize, with identical clearing. **MODIFIED** the graph table's
  filter row — the dependency filter moves to `Shift+F`. **ADDED** "Performance-view
  label-scale keys".
- `pane-class-layout`: **MODIFIED** "Non-latching maximize" — `z` and `f` both toggle
  it, with identical clearing.
- `signal-flow-graph`: **MODIFIED** "Upstream dependency subgraph" — the filter key is
  `Shift+F`.
- `performance-view`: **ADDED** "Performance-view zoom" and "Labels scale with the
  view zoom"; the fixed-size callout constant becomes the 100 % reference rather
  than the whole story.
- `configuration`: **MODIFIED** "Labels table in user configuration" — adds
  `performance_scale`, default 1.0, clamped 0.5..=4.0.

## Impact

- `src/handler.rs` — the global `f` arm in `handle_layout_pane_key`, the module-UI
  fit re-homing in `handle_physical_surface_key`, the graph filter key in
  `handle_graph_filter_key`, the `g f` arm in `handle_prefix_key`, and the
  performance label-scale steps in `handle_layout_adjust_key`.
- `src/gui/physical.rs` — `performance_mapping` takes the zoom and clamps;
  `CalloutSpec.label_size`; `callout_size` takes the size; `paint_callouts` reads it
  per spec; the overlay composes `14 × min(zoom, 2) × performance_scale`.
- `src/app.rs` — `performance_label_scale` state and its step helper.
- `src/config.rs` — `[labels] performance_scale` (a float field, so `Labels` loses its
  `Eq` derive); `src/main.rs` seeds it once at startup.
- `src/help.rs` — the module-UI and graph tables, with the parity test updated.
- `docs/keys.md`, `docs/views.md`, `docs/config.md`; regenerated `ARCHITECTURE.md` and
  `DESIGN.md`; re-blessed performance-view screenshot.
- `src/regression.rs` — end-to-end stories for `f`, the re-homed keys, the performance
  zoom and the label scale.
- No new theme token, no new dependency, no state machine: the zoom is the existing
  `physical_zoom` and the scale is one clamped `f32`.

## Non-goals

- No latching maximize: `f` is an alias of `z`, so `Tab` and `Esc` still restore the
  arrangement. A persistent single-window mode would need its own state and would
  contradict `pane-class-layout`'s existing clearing rule.
- No zoom in the graph, the source viewer or the optimizer; no label scaling outside
  the performance view (graph node labels, cluster titles and module-UI cell labels
  keep their fixed sizes).
- No panning in the performance view: the zoom clamp makes it unnecessary.
- No change to the label resolution chain, the drop/stack order in `place_labels`, the
  module UI's own fits beyond their new keys, or the pane arrangement rules.
- No new configuration for the performance zoom — it is the existing `[physical] zoom`.
