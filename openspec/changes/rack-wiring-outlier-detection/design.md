# Design: rack-wiring-outlier-detection

## Context

`droid_tui` already parses patches into `HwComponent` (with `controller`, `module_instance`, token id) and builds a `cable_index` plus a signal-flow graph with topology validation (`validate_topology` in `graph.rs`). It already surfaces topology findings via the `graph_edge_error` red token. See proposal.md for the motivation; the specs define the observable behavior this design implements.

The gap: validation today is cable-level (`n → 1`), not physical-geometry-level. This change adds element-precise rack geometry and flags far-and-direct bindings.

## Goals / Non-Goals

**Goals:**
- A static, machine-readable rack geometry (`rack_geometry.json`) that maps tokens to grid positions.
- A geometry feature extractor that reuses the existing parser and cable index.
- Track 1: a hard geometry invariant surfaced through the existing topology-warning + red-token path.
- Track 2: a spike that empirically decides whether a learned detector beats the hard threshold.

**Non-Goals:**
- No schema validation against the full DROID circuit catalog.
- No LSP/editor integration.
- No live hardware read; the geometry is a static rack description.
- No ML in the shipped binary unless the spike proves it beats Track 1.

## Decisions

### D1: `rack_geometry.json` as a single source of truth
A project-local JSON file holds controller positions and element offsets. Alternatives considered: hardcoding in the data factory, or deriving from the TUI panel layout. A shared file wins on DRY — metadroid, the data factory, and (optionally) the TUI all read the same table, so a rack change updates one place. The TUI panel layout is screen-space, not hardware-space, so it is deliberately not used as geometry truth.

**Schema (conceptual):**
```json
{
  "unit": "b32_pitch",
  "racks": [
    { "id": "R1", "y": 0, "controllers": [
      { "name": "R2C", "x": 0, "grid": "r2c" },
      { "name": "E4",  "x": 14, "grid": "e4" },
      { "name": "B32", "x": 30, "grid": "b32" }
    ]},
    { "id": "R2", "y": 1, "controllers": [ ... ] }
  ],
  "grids": {
    "b32": { "kind": "matrix", "cols": 8, "rows": 4, "row_wise": true },
    "e4":  { "kind": "stack", "count": 4, "pitch_y": 2 },
    "r2c": { "kind": "singleton" }
  }
}
```
Co-located `L→B` pairs resolve to the same grid cell (distance 0); mirrored controller names (`B32`/`b32`, `E4`/`e4`) reference the same grid.

### D2: Geometry lives in a new `src/geometry.rs` module
Keeps `patch.rs` focused on parsing and `graph.rs` on topology. `geometry.rs` owns the table load, token→grid resolution, and `BindingFeatures` computation. It has no terminal dependency, so it is unit-testable without rendering (matching the graph/layout module convention).

### D3: `BindingFeatures` shape (Track 1 + Track 2 shared)
A single feature struct feeds both the hard invariant and the learned spike, so the spike reuses the same extraction code rather than duplicating it:
```rust
struct BindingFeatures {
  src_kind: u8, sink_kind: u8, param_key: u8,
  src_xy: (u8, u8), sink_xy: (u8, u8),  // B32-grid units
  euclidean: f32, manhattan: u8,
  same_controller: bool, same_rack: bool,
  adjacent: bool,
  cable_hops: u8,
}
```

### D4: Track 1 — hard invariant in `graph.rs::validate_topology`
A new check: `euclidean > THRESHOLD && cable_hops == 0` → `TopologyIssue(Warning)`. It reuses the existing `TopologyIssue`/`graph_edge_error` plumbing, so the renderer change is minimal. Alternatives: a separate warning channel — rejected as over-engineering; the existing topology-warning path already colors the offending edge.

**Threshold:** `DISTANCE_THRESHOLD` (start ~8 grid units, tunable) — large enough that adjacent B32 buttons (distance 1) and same-controller stacks are never flagged, small enough to catch cross-rack direct wires.

### D5: Track 2 — spike, not shipped by default
A tiny autoencoder (`32→8→32`) trained on good-patch `BindingFeatures`, thresholded on reconstruction error, evaluated by ROC against held-out real patches and injected bads. It runs as an offline script (Python sklearn/torch), not in the binary. Productization is gated on beating Track 1 on the same corpus; if it does not, it is dropped (YAGNI).

## Risks / Trade-offs

- [Rack geometry is machine-specific] → Mitigation: `rack_geometry.json` is a static, user-editable description; Track 1's threshold is conservative so a slightly-off table under-flags rather than floods with false positives.
- [Hard threshold false positives (a legitimately far direct wire)] → Mitigation: default threshold conservative + `adjacent`/`same_controller`/`same_rack` features keep near wires unflagged; the spike's ROC quantifies the trade-off.
- [Data starvation for Track 2] → Mitigation: MPFS/metadroid expand the small real corpus to ~2k goods and inject bads; validation uses held-out real patches, not synthetic, so generator drift is caught.
- [Track 2 never beats Track 1] → Mitigation: explicitly a spike; drop the model, keep Track 1. Not a failed change.

## Migration Plan

No runtime migration. `rack_geometry.json` is additive. Track 1 ships in the normal `graph.rs` validation path; Track 2 is an offline script. Rollback = remove the new validation block.

## Open Questions

- The exact `DISTANCE_THRESHOLD` value — resolved by tuning against the real corpus during Track 1 implementation; does not change the spec or the approach.
