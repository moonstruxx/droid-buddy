# Proposal

## Why

Every DROID rack contains exactly one master module — MASTER or MASTER18 — and which
one it is changes what a patch can address: the manual states that the MASTER "has
eight CV inputs and eight CV outputs" while "the MASTER18 does not have CV inputs,
but instead it has two trigger/gate inputs called I1 and I2 and four trigger/gate
outputs called G1 .. G4", that `vcotuner` "does not exist on the MASTER" and that
`sinfonionlink` is absent from it too.

The patch carries that choice. Forge 9.2: *"You need to select your type of master
in the menu Rack … Your selection is saved as a comment in the patch"* — and real
patches in this repository carry it as `# LABELS: master=18` (16 files) or
`# LABELS: master=16`.

The app currently draws a master faceplate for every patch, but it does so by
synthesis and by guesswork:

- `PhysicalLayout::build` unconditionally pushes a master module at chain position 0,
  choosing the model from `Patch::master_requirement()` — *"master18 iff any CV token
  number > 8"*. That rule is wrong in both directions: a patch using `I1` plus `O9`
  resolves to MASTER18, which has no CV inputs at all, and a patch using only
  `[vcotuner]` resolves to MASTER, where that circuit does not exist.
- `# LABELS: master=NN` is read by nothing. `parse_preamble_labels` drops the line
  because `LABELS` is not a hardware token, so the one authoritative signal in the
  file is discarded.
- Nothing reports the case the manual describes when you switch masters — *"some new
  patch problems might be shown, others might vanish"*. A patch no master can host,
  and a patch whose declared master cannot host it, both load and draw silently.
- `ram_overflow` warns once per schema master (`master16`, `master18`), naming a
  model the patch does not use.

## What Changes

- **One master vocabulary.** `MasterRequirement` becomes `MasterModel` (`Master`,
  `Master18`) carrying `geometry_key()` (`master`/`master18`), `ram_key()`
  (`master16`/`master18`) and `label()` (`MASTER`/`MASTER18`) — the three names the
  codebase currently uses for one fact collapse into one enum.
- **The declaration is parsed.** The preamble comment `# LABELS: master=16|18`
  becomes `Patch.master_declaration` (`16` → MASTER, `18` → MASTER18; first
  occurrence wins, any other value ignored, never treated as a hardware label).
- **Resolution becomes a pure model.** New `src/rack.rs` resolves a patch against the
  schema into a `MasterPlan { model, source, conflicts }`: a declaration is
  authoritative; without one the plan infers a model only when exactly one can host
  the patch, and otherwise defaults to MASTER. The capability rules come from the
  manual and the embedded geometry: `I3..I8` and `I1`/`I2` used as CV require MASTER;
  `vcotuner` and `sinfonionlink` require MASTER18; `O1..O8` are hostable by both.
- **The module UI draws the resolved master.** `PhysicalLayout::build` takes the
  model instead of inferring it, `App` holds the plan after a load, and the module-UI
  pane title names the model it drew.
- **The fit is reported.** `master_conflict` (`Warning`) names a use the resolved
  model cannot host and the model that would host it; `no_master_fits` (`Error`)
  reports a patch that neither model can host. A declared conflict stays loadable —
  the Forge shows problems rather than refusing the patch — while an unhostable
  patch gates the load like every other Error.
- **RAM is judged against one master.** `ram_overflow` compares the patch's circuit
  RAM against the resolved model's budget only, so one finding names the master that
  actually applies.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `patch-parsing`: a new requirement for the preamble master declaration — the parser
  exposes `# LABELS: master=16|18` as the patch's master declaration and keeps it out
  of the hardware-label map.
- `patch-validation`: a new requirement for master resolution and fit
  (`master_conflict`, `no_master_fits`, the resolution default, and the load status
  naming the model and its source); `Nine LSP-equivalent checks with typed severity`
  changes so `ram_overflow` is judged against the resolved master's budget.
- `physical-scale-model`: `Physical grid model` changes so the chain's master member
  — exactly one, at chain position 0 — is the master model resolved for the patch
  rather than a fixed fallback.
- `module-ui`: `Master faceplate` changes so exactly one master faceplate renders, of
  the resolved model, and the pane title names which one.

## Impact

- Touched: `src/patch.rs` (`MasterModel`, declaration parsing, `Patch` field),
  new `src/rack.rs` (resolution + fit rules), `src/lib.rs` (module wiring),
  `src/physical.rs` (`build` takes the model), `src/validation.rs` (two checks, RAM
  rebinding), `src/app.rs` (plan on load, load-status segment),
  `src/gui/{physical,mod}.rs` (thread the model, pane-title segment),
  `src/regression.rs`, `fixtures/validation/*.ini`, `docs/views.md`,
  `docs/labels.md`.
- Visible behaviour: the drawn master follows the patch's declaration instead of a
  CV-number guess; the pane title and the load status say `MASTER`/`MASTER18` and
  whether it was declared or inferred; two new findings appear in the validation
  modal; the RAM finding names one master instead of two.
- Unchanged: `controller_geometry.json`, `rack_geometry.json`, config keys
  (`[physical.rack]` and its `CV I/O` assign key keep working), keybindings, the
  label store, plugin loading, the `.ini` writer, and every published hit rect.
- Breaking for consumers: `MasterRequirement` and `Patch::master_requirement()` are
  removed; `PhysicalLayout::build` gains a required parameter. Both are crate-internal.

## Non-goals

- No gate (`G`) and no LED-register (`R`) fit. Neither is an `hw_component` token —
  `token_kind` models `B/L/P/M/O/I/E/S` only — so `MASTER18`'s `G1..G4` and the
  MASTER's 16-register LED matrix stay outside the model rather than being guessed at.
- No expander modelling: a MASTER's MIDI circuits still need an X7 in hardware, and
  `G8` gate counts are not checked. Expanders are a second rack dimension, not a
  master-model question.
- No inference from circuit usage when a patch declares nothing and both models could
  host it beyond the single-model case: the default is MASTER and the fit reports the
  alternative.
- No writing of the declaration back to `.ini`, no new config key, no new keybinding,
  no persistence of the plan.
- No hand-edits of `ARCHITECTURE.md` or `DESIGN.md`; those regenerate at archive.
