# Tasks

## 1. Model, declaration, and resolution

- [x] 1.1 Replace `MasterRequirement` with `MasterModel` (`geometry_key`/`ram_key`/`label`, serde for the `Patch` field) and update `Patch::sample` plus every `Patch` literal; keep the `>8 CV jack` function returning the new type so the crate stays green — 1.4 deletes it; verify with `cargo test patch::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs, src/test_support.rs] -->
- [x] 1.2 Parse `# LABELS: master=16|18` from the preamble block into `Patch.master_declaration` (`16`→MASTER, `18`→MASTER18, first wins, unknown ignored, not read after the first section); add the in-module tests including "the declaration never becomes a preamble label"; verify with `cargo test patch::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs] -->
- [ ] 1.3 Add the pure `src/rack.rs` with `MasterPlan{model,source,conflicts}`, `MasterSource`, `MasterConflict` and `resolve(&Patch,&Schema)` over the verified capability matrix (`I3..I8` → MASTER, `I1`/`I2` on a `cv`-typed param → MASTER, `vcotuner`/`sinfonionlink` → MASTER18); wire it in `src/lib.rs` and unit-test declared, inferred, defaulted, conflicting and unhostable plans; verify with `cargo test rack::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2], touches: [src/rack.rs, src/lib.rs] -->
- [ ] 1.4 Make `PhysicalLayout::build(patch, model)` take the model instead of resolving it, sweep every in-module call site, and re-fit `deep_cv_patch_uses_master18_geometry` (it asserts the deleted `O9` rule) onto a declaration plus a silent-patch → MASTER case; verify with `cargo test physical::`. <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/physical.rs] -->
- [ ] 1.5 Store `App.master_plan` in `load_patch` and thread it through `gui::physical::physical_spec` into the build call, so the drawn faceplate can never disagree with the validated verdict; verify with `cargo test gui::physical::`. <!-- agent: frontend-engineer, depends_on: [1.4], touches: [src/app.rs, src/gui/physical.rs] -->

## 2. Rack fit in validation

- [ ] 2.1 Emit `master_conflict` (`Warning`) from `MasterPlan.conflicts` — span at the offending entry or section header, message naming the model that would host it — and `no_master_fits` (`Error`, naming both uses) when the plan cannot resolve; verify with `cargo test validation::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.3], touches: [src/validation.rs] -->
- [ ] 2.2 Add `fixtures/validation/master_conflict_18.ini`, `master_conflict_circuit.ini` and `master_no_fit.ini` with the check-matrix rows asserting span, severity and code, and that the two conflict fixtures still load while only the no-fit fixture gates; verify with `cargo test validation::` and `cargo test regression::`. <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/validation.rs, src/regression.rs, fixtures/validation/*.ini] -->
- [ ] 2.3 Bind `ram_overflow` to the resolved master's `ram_key()` — one finding naming that model — and update the tests that currently expect a finding per schema master; verify with `cargo test validation::` and `cargo test regression::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [2.1], touches: [src/validation.rs, src/regression.rs] -->
- [ ] 2.4 Add regression stories over the real declaration families: a declared-18 patch addressing `I1..I7` is reported and still loads, a declared-18 gate-input patch (motokit shape) is clean, and a declared-16 patch with `I1..I8` is clean; verify with `cargo test regression::`. <!-- agent: horst-engineer.build, depends_on: [2.2, 2.3], touches: [src/regression.rs] -->

## 3. Surfacing and documentation

- [ ] 3.1 Name the resolved model in the module-UI pane title (`Module UI · … · MASTER18`) and assert the emitted label text in the surface test; verify with `cargo test gui::`. <!-- agent: frontend-engineer, depends_on: [1.5], touches: [src/gui/mod.rs, src/gui/physical.rs] -->
- [ ] 3.2 Name the model and its source in the load status (`Loaded x — MASTER18 (declared)` / `MASTER (inferred)` / `MASTER (default)`) while keeping the existing `Loaded`/`Load failed` prefix assertions green; verify with `cargo test app::`. <!-- agent: rusty-engineer.build, depends_on: [1.5], touches: [src/app.rs] -->
- [ ] 3.3 Document the master faceplate, the `# LABELS: master=NN` Forge convention, and both finding codes with their severities in `docs/views.md` and `docs/labels.md`; verify by reading the rendered pages. <!-- agent: devops-engineer.fast, depends_on: [3.1], touches: [docs/views.md, docs/labels.md] -->

## 4. Verification

- [ ] 4.1 Run the gate at CI parity — `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, the release perf gate and the headless `--self-test` — and confirm every command exits 0. <!-- agent: horst-engineer.fast, depends_on: [2.4, 3.2, 3.3], touches: [] -->
