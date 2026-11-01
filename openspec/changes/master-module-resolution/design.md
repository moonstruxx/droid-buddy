# Design

## Context

See `proposal.md` — Why. The constraints that shape the approach:

- The two master models are embedded geometry entries with different faces:
  `master` is 8 HP with a 4×4 CV grid (`I1..I8` + `O1..O8`), a 4×4 LED matrix
  (`R1..R16`) and the SD button; `master18` is 6 HP with `O1..O8` plus
  `Gin1`/`Gin2`/`Gout1..Gout4` and the SD button, and no LED matrix. Manual 2.4 fixes
  the semantics: MASTER18's `I1`/`I2` are trigger/gate inputs that only know low/high,
  and its `G1..G4` are gate outputs. Manual refs: `sinfonionlink` — "The MASTER does
  not have this circuit"; `vcotuner` — "This circuit does not exist on the MASTER";
  `outputcalibrator` exists on both.
- The schema carries per-master RAM budgets under `available_memory`
  (`master16`, `master18`); `validation.rs` currently iterates all of them and emits
  one finding per exceeded budget.
- `Patch::master_requirement()` (patch.rs) is the only resolver today; its sole
  consumer is `PhysicalLayout::build`, which unconditionally pushes the master module
  at chain position 0 and takes its geometry key from that answer.
- Parsing is schema-free and must stay so: `patch.rs` imports neither `schema` nor
  `validation`. Validation owns schema access; `physical.rs` currently avoids it too.
- `parse_preamble_labels` already ignores `# LABELS: master=18`, because
  `is_preamble_token` accepts hardware tokens only. That is the seam the declaration
  parser uses, and it must not regress into a label.
- Gates and LED registers are not `hw_component` tokens (`token_kind` has no `G` or
  `R` branch), so the fit cannot see them without inventing a token model.

## Goals / Non-Goals

**Goals:**

- One resolution step decides the master model for a patch, and every consumer (the
  module UI, the RAM verdict, the findings, the status) reads that one answer.
- The declaration is authoritative; silence has a *named* default rather than a
  guessed one.
- Both failure modes the hardware has are observable: a declared master that cannot
  host the patch, and a patch that no master can host.

**Non-Goals:**

- Modelling gate jacks (`G`), LED registers (`R`) or expanders (`G8`, `X7`) — see
  `proposal.md` Non-goals. The fit covers CV jacks and circuits only.
- Replacing `controller_geometry.json`'s master entries or renaming the chain's
  `CV I/O` controller/assign key.
- A settings surface for the master (no config key, no keybinding).

## Decisions

### D1 — One `MasterModel`, three vocabularies collapsed

`MasterRequirement` and its `geometry_key` string pair become:

```rust
pub enum MasterModel { Master, Master18 }
impl MasterModel {
    pub fn geometry_key(self) -> &'static str  // "master" | "master18"
    pub fn ram_key(self) -> &'static str       // "master16" | "master18"
    pub fn label(self) -> &'static str         // "MASTER" | "MASTER18"
}
```

Named consumers (the repo rejects speculative types): `physical.rs` uses
`geometry_key()`, the RAM check uses `ram_key()`, the pane title and load status use
`label()`, and `Serialize`/`Deserialize` exist because `Patch` derives them and
carries the field. Kept as a plain enum: no `Display`, no `FromStr`, no config
parsing — the only text form in play is the Forge's `16`/`18`, which the declaration
parser matches explicitly.

*Alternative considered:* keep `MasterRequirement` and add conversion helpers. Rejected:
the three vocabularies are exactly the drift this change removes, and `Master`/`master`
/`master16` already disagree in name today.

### D2 — The declaration lives in the preamble comment block, not in labels

`# LABELS: master=16|18` is parsed by a dedicated function over `raw_lines`, only
before the first section header, first occurrence wins, `16` → MASTER, `18` →
MASTER18. Any other value is ignored **without a finding**: the field belongs to the
Forge, older or newer Forge versions may use it differently, and a patch must not fail
to load because of metadata the app only reads. The line must never reach
`parse_preamble_labels` — the existing token filter already refuses `LABELS`, and a
test pins that so a future token addition cannot turn it into a label.

*Alternative considered:* reuse the label map with a `LABELS` key. Rejected: it would
surface a pseudo-token in the label overlay and the sidebar, and it conflates patch
metadata with hardware naming.

### D3 — Resolution is one pure function over patch + schema

```rust
pub struct MasterPlan {
    pub model: MasterModel,
    pub source: MasterSource,          // Declared | Inferred | Default
    pub conflicts: Vec<MasterConflict>,
}
pub fn resolve(patch: &Patch, schema: &Schema) -> MasterPlan
```

Rules, derived from the verified capability matrix:

| Use | Requires |
|---|---|
| any `I{n}` with `n >= 3` | MASTER (MASTER18 has no such jack) |
| `I1`/`I2` bound to a `cv`-typed param | MASTER (MASTER18 reads them as low/high) |
| section `vcotuner` or `sinfonionlink` | MASTER18 |
| `O1..O8` | either |

Resolution order: a declaration wins (`source = Declared`); otherwise, if exactly one
model satisfies every requirement, infer it (`Inferred`); otherwise, if no model
does, the plan reports `no_master_fits`; otherwise `Default` = MASTER, because MASTER
hosts every CV jack MASTER18 does and more. `physical.rs` stays schema-free: it
receives the plan's model as a parameter instead of resolving it.

*Alternative considered:* resolve inside `Patch` (schema-free, so no circuit rule) or
inside `physical.rs` (duplicating schema access). Both split the answer across
consumers, which is the defect this change fixes. `validate_patch(&Patch, &Schema)`
keeps its signature by calling `resolve` internally; `App::load_patch` calls it once
and stores the plan for the renderer.

### D4 — Severity: a conflict warns, an unhostable patch errors

`master_conflict` is `Warning`; `no_master_fits` is `Error`. The Forge's own behaviour
is the precedent — switching the master shows *problems*, it does not refuse the
patch — and this app is a patch viewer: a declared-18 patch that addresses `I3..I7`
must stay inspectable, with the finding naming the jack and the model that would host
it. `no_master_fits` is different in kind: neither model can run the patch, so it
joins the Errors that gate a load.

*Alternative considered:* `Error` for every conflict. Rejected: it makes the real
declaration families in `patches_remote/` and `ext/droid-lsp/` unopenable.

### D5 — No param typing for `I1`/`I2` beyond `cv`

The schema's jack/param types distinguish `cv` from `gate`/`trigger`, so `I1`/`I2`
need no heuristic: a `gate`/`trigger`-typed param on `I1` is a legitimate MASTER18
patch (`motokit.ini` binds them to `[CLK In]`/`[RST In]` and declares `master=18`),
while a `cv`-typed param needs MASTER. `I3..I8` need no typing at all — MASTER18 has
no third responsive jack.

*Alternative considered:* treat every `I` token as MASTER-only. Rejected: it would flag
every genuine MASTER18 MIDI/clock patch.

### D6 — RAM is judged against the resolved master

`ram_overflow` compares the summed circuit RAM against
`plan.model.ram_key()`'s budget and emits one finding that names that master. A patch
over MASTER's budget but under MASTER18's now reports one finding against MASTER
instead of two contradictory ones; if the fit also forces MASTER18, the resolution
already accounts for it.

*Alternative considered:* keep per-master budgets. Rejected: the message names a model
the patch does not use, and the user cannot tell which verdict applies.

### D7 — `PhysicalLayout::build` takes the model

`build(patch: &Patch, model: MasterModel)`: explicit parameter, so the compiler lists
every call site (including the in-module fixtures) and no path can silently keep the
old guess. The `O9`-heuristic test (`deep_cv_patch_uses_master18_geometry`) is re-fitted
onto a declaration — it asserts the rule being deleted. The chain keeps
`controller: "CV I/O"` and its `None` instance so `RackSpec.assign` keys and the
existing ledger tests are untouched.

*Alternative considered:* keep `build(&Patch)` and read `patch.master_model()`. Rejected:
that is the schema-free half of the resolution, and it would let the drawn faceplate
disagree with the validated verdict.

## Risks / Trade-offs

- [The declaration is Forge metadata and can be wrong] → declared wins, the conflict
  finding names the jack or circuit; nothing is silently re-plated.
- [Real patches in this repo declare `master=18` while addressing `I1..I7`] → they
  load with `master_conflict` warnings; the regression stories pin that they are
  reported and still inspectable.
- [Gates and LED registers are outside the fit] → documented as a non-goal, so a
  master18 patch that writes `R1..R16` is not flagged; the model's boundary is stated
  in the spec rather than implied.
- [A new required parameter on `build` touches many fixtures] → mechanical, and the
  compiler enumerates them; the tasks carry the call-site sweep explicitly.
- [`no_master_fits` gates a load] → only fires when the patch needs `I3..I8` *and* a
  MASTER18-only circuit; a wrong ampersand in the rule would be caught by the fixture
  matrix, not at runtime.

## Migration Plan

No stored state, no config key and no `.ini` rewrite: the change is additive for
patches (a declaration that resolves to MASTER, the current default, changes nothing
visible) and a rename inside the crate. Rollback is a revert; nothing is persisted.

## Open Questions

- Whether newer Forge versions write master values other than `16`/`18`. Ignored today
  by design (D2); answering it only widens the accepted set, which does not change the
  specs, the approach or the task breakdown.
