# Tasks

## 1. Inventory in configuration

- [x] 1.1 Add `RackRow.modules: Vec<String>` and the module-key parser in `src/physical.rs` — `"{controller} {instance}"` with a 1-based instance, a bare controller for single-instance modules, `master`/`CV I/O` as the master alias, and an unknown controller warning once and being skipped — with `#[serde(default)]` so existing configs load unchanged; unit-test the parser, the alias and the skip; verify with `cargo test physical::`. <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/physical.rs] -->
- [x] 1.2 Sanitise the parsed inventory in `src/config.rs` (trim entries, drop empties, lowercase the controller part, keep the instance) and round-trip it through load/save so a saved config re-reads identically; verify with `cargo test config::`. <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/config.rs] -->

## 2. Chain and rack from the inventory

- [x] 2.1 Change `PhysicalLayout::build` to `build(patch: Option<&Patch>, model: MasterModel, inventory: &[ModuleKey])` with an `inventory_driven` flag on `PhysicalLayout`: inventory modules first in row/list order, patch-only modules appended in declaration order, a module the patch does not address keeping an empty component list, instance keys binding that instance's components; update every call site; verify with `cargo test physical::`. <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/physical.rs] -->
- [x] 2.2 Make `RackLayout::pack` place row-listed modules in their own row in list order (`assign` only for unlisted modules) and let `RackSpec::default_case` derive its width from an inventory-only chain; verify with `cargo test physical::`. <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/physical.rs] -->
- [x] 2.3 Let `App::physical_chain_and_rack`, `fit_physical_view`, `fit_physical_view_rack` and the fit latch work when an inventory is configured and no patch is loaded (master model from `master_model()`, empty labels/states); verify with `cargo test app::`. <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [src/app.rs] -->
- [x] 2.4 Let `gui::physical::physical_spec` return `Some` for an inventory with no patch and publish no `component_rects` in that state; verify with `cargo test gui::physical::`. <!-- agent: frontend-engineer, depends_on: [2.3], touches: [src/gui/physical.rs] -->

## 3. Unaddressed emission

- [x] 3.1 Switch the `CV I/O` suppression in `rack_geometry` to the chain flag: unaddressed cells are emitted for every module when the chain is inventory-driven and stay suppressed for the master on a patch-derived chain; verify with `cargo test gui::physical::`. <!-- agent: frontend-engineer, depends_on: [2.4], touches: [src/gui/physical.rs] -->
- [x] 3.2 Pin the counts with tests: without an inventory the master front is byte-identical to today; with one, MASTER exposes 33 cells and MASTER18 15, G8 16, X7 13, R2C 2; no patch with an inventory publishes zero hit rects; verify with `cargo test gui::physical::` and `cargo test regression::`. <!-- agent: horst-engineer.build, depends_on: [3.1], touches: [src/regression.rs, src/gui/physical.rs] -->

## 4. Element identity in geometry

- [x] 4.1 In `tools/acquire_geometry.py`, rename MASTER18's gate cells to `I1`, `I2`, `G1..G4` and give the R2M/R2C serial jacks explicit element numbers with `addressable: false`; regenerate `controller_geometry.json` and confirm `python3 tools/acquire_geometry.py --check` reports OK. <!-- agent: data-engineer, depends_on: [], touches: [tools/acquire_geometry.py, controller_geometry.json] -->
- [x] 4.2 Add optional `element`/`kind_letter` fields to `ElementCellEntry` in `src/physical.rs` and prefer them over the label-derived identity in `preprocess_cells`; unit-test both routes plus a cell that has neither; verify with `cargo test physical::`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [4.1], touches: [src/physical.rs] -->
- [x] 4.3 Pin the resolution with tests: `cell_for("I1")`, `cell_for("G1")` and `cell_for("G4")` on MASTER18 hit the gate column (not `O1`), an R2C faceplate yields exactly two cells, and a MASTER18 patch addressing `I1`/`I2` renders on the right jacks; verify with `cargo test regression::` and `cargo test physical::`. <!-- agent: horst-engineer.build, depends_on: [4.2], touches: [src/regression.rs, fixtures/*] -->

## 5. Documentation and gates

- [x] 5.1 Document the inventory in `docs/config.md` (`modules` on `[[physical.rack.rows]]`, the module-key vocabulary, the master alias, unknown-entry warning, precedence against `assign`) and the rack-first view in `docs/views.md` (what an inventoried rack shows, what stays dim, the no-patch rack); verify by reading the rendered pages. <!-- agent: devops-engineer.fast, depends_on: [2.2], touches: [docs/config.md, docs/views.md] -->
- [x] 5.2 Run the gate at CI parity — `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, the release perf gate and the headless `--self-test` — and confirm every command exits 0. <!-- agent: horst-engineer.fast, depends_on: [3.2, 4.3, 5.1], touches: [] -->
