## 1. Wire the catalog into the cable index

- [x] 1.1 Replace the `key_lower == "output"` producer test in `collect_cable_index` (src/patch.rs) with `load_schema().get_param_kind(&section.name, &key_lower)`, keeping the bare-`_NAME`-vs-expression rule and the `output` fallback for unknown circuits and parameters. Verify: a new in-module test in src/patch.rs proves a catalog output parameter with a non-`output` key produces a source, a catalog input parameter produces a sink, and an unknown circuit keeps the `output` convention. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/patch.rs] -->

## 2. Wire the catalog into the output reverse map

- [x] 2.1 Replace the `k.to_lowercase() == "output"` test in `collect_circuit_outputs` (src/patch.rs) the same way so the influence reverse map agrees with edge direction. Verify: existing influence tests stay green and a catalog output parameter with a non-`output` key appears in a section's `circuit_outputs`. <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/patch.rs] -->

## 3. Regression and full gate

- [x] 3.1 Add a graph-level regression test that builds the graph from a schema-level fixture whose source uses a catalog output parameter and whose sink uses a catalog input parameter, and asserts the edge direction matches the catalog; keep every existing fixture byte-identical. Verify: `cargo test` passes. <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/patch.rs, src/graph.rs] -->
- [ ] 3.2 Run the full verification gate and accept any intended snapshot changes. Verify: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, and `cargo build --release --locked` all exit 0. <!-- agent: horst-engineer.fast, depends_on: [3.1], touches: [] -->
