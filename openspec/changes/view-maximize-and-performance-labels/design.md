# Design

## Context

See `proposal.md` — Why. Five facts shape the approach.

1. `handle_event` is an ordered chain (`src/handler.rs`): overlays (help,
   validation, label editor) → picker → armed `g` prefix → `handle_layout_pane_key`
   → per-view tables → `handle_shared_key`. `z` already lives in
   `handle_layout_pane_key` and calls `App::maximize_toggle`, whose clearing rules
   (focus change, `Esc`) are enforced elsewhere in the chain.
2. Shifted glyphs arrive as their symbol (`KeyEvent::from_winit_parts`): `Shift+F`
   is `Char('F')`, `Shift+-` is `Char('_')`, and on a German layout `Shift++` is
   `Char('*')` while on US it is `Char('+')` with the shift flag set. Existing
   bindings (`'C'`, `'R'`, `'U'`) already rely on this.
3. `g` is an armed prefix with a one-second lazy timeout; its follow-ups are
   matched in `handle_prefix_key` and each opens something or toggles a mode.
4. The performance view's geometry is `performance_mapping(rack, pane)`, which
   returns `fit × 0.5` centred and ignores `physical_zoom`. `spec_mapping` already
   folds the pane origin and falls back to the normal mapping on degenerate input.
5. Callout size is one constant, `CALLOUT_LABEL_SIZE = 14.0`, consumed twice:
   `callout_size` measures the placement budget fed to `performance::place_labels`,
   and `paint_callouts` draws the headline and the `0.8 ×` state row. `paint_callouts`
   is shared with the label annotations and the hover reveal.

## Goals / Non-Goals

**Goals:**
- One maximize action with one implementation and two keys.
- The key map stays derivable: every key `f` displaces moves to a place that
  already means something similar (`Shift` variants stay pane-scoped, `g` carries
  the rack-fit sibling of the labels fit).
- Label size has one source per frame: placement and paint consume the same number.
- The performance view's zoom is the existing zoom, so nothing new is persisted for
  it and the two surfaces cannot disagree.

**Non-Goals:**
- No new state machine for the band; maximize keeps `PaneLayout.maximized`.
- No pan in the performance view, and no re-centring logic beyond the mapping.
- No label scaling outside the performance callouts.

## Decisions

### D1 — `f` maximizes in the layout table, as an alias of `z`

`handle_layout_pane_key` is the one place that runs before any per-view table and
after overlays/picker/prefix, which is exactly the precedence a band-level key
needs: the picker keeps its favourite toggle, the help modal keeps its contents, and
the graph and module UI lose their `f` arms. The arm calls the same
`App::maximize_toggle` as `z`, so the non-latching rules (a second press, `Tab`,
`Esc`) are inherited rather than re-implemented.

*Alternatives considered.* Keeping `f` pane-scoped and adding the maximize under a
new key was the user's alternative; rejected because the request is precisely that
the obvious key maximize from anywhere. A separate latching "single window" mode was
rejected: it would need its own state, its own clearing rules and a contradiction
with `pane-class-layout`.

### D2 — Displaced keys go to `Shift+F` (pane-scoped) and `g f` (prefixed)

`Shift+F` is already a distinct glyph and both consumers are pane-scoped, so the
module UI's labels fit and the graph's dependency filter can share it without
collision — the same way `f` used to. The rack fit has no such counterpart and is
the rarer of the two module-UI fits, so it moves to the existing `g` prefix as
`g f`, beside `g v`/`g g`/`g o`/`g c`/`g s`. The prefix handler gains one arm; no new
top-level key is spent.

*Alternatives considered.* `Ctrl+F` for the rack fit was rejected because the picker
binds `Ctrl+F` as its filter toggle, so the same chord would mean different things
one overlay away. Dropping the rack fit entirely was rejected: `Shift+C` re-fits to
the labels fit, which on a rack with written labels is the overflowing view — the
overview would become unreachable.

### D3 — The performance zoom is `physical_zoom`, clamped to the rack fit

`performance_mapping(rack, pane, zoom)` keeps the compact base (`fit × 0.5`) and
multiplies it by the zoom, then clamps the result to `fit`. Consequences: 100 % is
byte-identical to today's picture; 200 % fills the pane with the rack; the clamp
makes panning unnecessary because the rack can never overflow; and the module UI and
the performance view share one persisted zoom, so leaving the performance view does
not change what the module UI shows. The wheel and the preset keys already write
`physical_zoom`, so both gestures start working in the view with no new input path.

*Alternatives considered.* A separate `performance_zoom` would need its own state,
its own persistence and its own clamp, and would let the two surfaces disagree about
"the zoom". Making the rack overflow and adding panning was rejected as scope the
view's own spec does not need.

### D4 — Effective label size = `14 × rel_zoom × performance_scale`

`rel_zoom = min(zoom, 2.0) / 1.0` against the compact base, so 100 % zoom gives 14 pt
and 200 % gives 28 pt; `performance_scale` is the persisted independent knob. The
product is computed once per frame in `performance_overlay` and written onto
`CalloutSpec.label_size`, then `callout_size` measures the placement budget with it
and `paint_callouts` builds its `FontId` from it. Two consumers, one number — the
alternative (scaling only at paint time) would place labels in rects measured for a
different size, which is exactly the overlap the placement model exists to prevent.
`CalloutSpec` also serves the annotations and hover paths, which pass the constant,
so those surfaces are unchanged by construction.

*Alternatives considered.* Scaling inside `paint_callouts` from a global flag —
rejected for the reason above. Making the scale a multiplier of the pane size —
rejected: the request is about zoom, and pane size is already implicitly handled by
`fit`.

### D5 — `Shift`+`+`/`-` bind by glyph, not by modifier

Because `from_winit_parts` yields the produced character, the fine pair binds
`Char('*')` (German `Shift++`), `Char('_')` (`Shift+-`, both layouts), and
`Char('+')`/`Char('-')` *with* the shift flag (US `Shift+=`). Plain `+`/`-` stay the
coarse/zoom path. On layouts where `+` is unshifted the two are distinct glyphs; on
US layouts plain `+` carries the shift flag, so it is read as the fine step and the
coarse up-step is `-`'s counterpart only — a layout-dependent asymmetry the docs
state rather than hide. 5 % steps with a 50–400 % clamp give fine control without a
ladder to memorise; the status line reports the percentage so each press is
observable.

*Alternatives considered.* Doubling every `+` press into fine/coarse phases — rejected
as invisible state. A dedicated `,`/`.` pair — rejected: the user asked for the shift
form, and `,`/`.` have no meaning to relate to.

### D6 — `performance_scale` is an `f64` under `[labels]`, so `Labels` loses `Eq`

`Labels` derives `Eq` today because its two fields are `bool` and `u8`. A float
necessarily drops that derive; its only consumers compare with `PartialEq`
(`latency.rs` builds `Labels::default()`), so the ripple is one derive line. The
value is clamped and warned on load like `max_shift_layer`, and an absent field means
1.0, matching the table's existing "absent means default" rule.

*Alternatives considered.* Storing an integer percent to keep `Eq` — rejected: the
file would read `performance_scale_percent = 115` for a value the UI talks about as
115 %, and the key name the user asked for is `performance_scale`.

## Risks / Trade-offs

- **[High zoom starves the label field]** → The rack may fill the pane while the
  labels need the space around it; callouts then overflow the pane and are clipped.
  Mitigation: the zoom clamp is the rack fit (not a multiple of it), and the base
  scale gives the operator a knob to trade label size back; the status line reports
  both numbers so the trade is visible.
- **[`f` and the picker]** → `f`/`F` toggle favourites inside the picker. Mitigation:
  the prefix/picker steps run before the layout step in the chain; a regression story
  pins the picker's meaning.
- **[Layout-dependent `Shift++`]** → On US layouts the coarse up-step can be
  unreachable. Mitigation: glyph-based binding documented in `docs/keys.md`; the fine
  step is available on every layout.
- **[Scale and placement interaction]** → A larger label can change the band wrapping
  in `place_labels`, so the label set can visibly reflow when the scale changes.
  Mitigation: the reflow is deterministic and covered by the existing invariance
  tests, which the new tests extend to a scaled run.
- **[Persistence of a float in a derived-`Eq` struct]** → `Labels` loses `Eq`.
  Mitigation: verified by the build; no consumer requires `Eq`.

## Migration Plan

No data migration. `[labels] performance_scale` is optional and absent means 1.0, so
existing config files load unchanged; the previous keys (`f` for the labels fit,
`Shift+F` for the rack fit, `f` for the graph filter) intentionally change meaning and
are documented in the help tables and `docs/keys.md`. Rollback is a revert of the
change: no file format breaks either way.
