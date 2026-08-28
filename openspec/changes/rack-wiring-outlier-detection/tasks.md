# Tasks: rack-wiring-outlier-detection

## 1. Rack Geometry

- [ ] 1.1 Create `rack_geometry.json` (R1 horizontal case + R2 mixed rack; B32 4×8 row-wise, vertical mount; E4/e4 stack, L→B co-located whitelist; `B32==b32`/`E4==e4` shared grids) and a validation test that resolves `B1.17`→row 2 col 0 and confirms co-located `L1.17`/`B1.17` distance 0 <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [rack_geometry.json, src/geometry.rs, src/geometry.rs#tests] -->
- [ ] 1.2 Implement `src/geometry.rs` (load `rack_geometry.json`, token→grid resolution, `BindingFeatures` with `src_xy`/`sink_xy`/`euclidean`/`manhattan`/`adjacent`/`same_controller`/`same_rack`/`cable_hops`) and verify geometry unit tests pass (far direct wire, adjacent pair, via-cable pair) <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.1], touches: [src/geometry.rs, src/lib.rs] -->

## 2. Track 1 — Hard Invariant

- [ ] 2.1 Wire geometry into `graph.rs::validate_topology`: flag `euclidean > DISTANCE_THRESHOLD && cable_hops == 0` as `TopologyIssue(Warning)`; reuse `graph_edge_error` red token in `ui.rs` render_graph; tune `DISTANCE_THRESHOLD` against real corpus <!-- agent: api-engineer.build, depends_on: [1.2], touches: [src/graph.rs, src/ui.rs] -->
- [ ] 2.2 Add Track-1 tests: MFS-drum `E4.4→M4` direct flagged; via-cable `E4.4→e4` not flagged; adjacent `B1.17→B1.18` not flagged; `L1.n→B1.n` whitelisted not flagged; verify `cargo test` passes <!-- agent: horst-engineer.build, depends_on: [2.1], touches: [src/graph.rs#tests, fixtures/mfs_drum.ini] -->

## 3. Track 2 — Data Factory + Spike

- [ ] 3.1 Build data factory: expand good corpus (acidified/deepsea/copycat) via MPFS generator + metadroid programmatic assembly to ~2k good variants; inject synthetic bads (E4.4→M4, encoder→button, far B1→B32); emit `BindingFeatures` CSV <!-- agent: dermannmitdermachine-engineer.build, depends_on: [1.2], touches: [tools/**, scripts/**, corpus/**] -->
- [ ] 3.2 Spike: train tiny autoencoder (32→8→32) on good features, threshold on reconstruction error, compute ROC on held-out copycat + injected bads; report whether it beats Track 1's hard threshold <!-- agent: rusty-engineer.build, depends_on: [3.1, 2.1], touches: [spike/**, results/] -->
- [ ] 3.3 Decision: if the spike beats Track 1, add a follow-up bead to productize the model; if not, document why and keep Track 1 only (YAGNI) <!-- agent: rusty-engineer.fast, depends_on: [3.2], touches: [] -->
