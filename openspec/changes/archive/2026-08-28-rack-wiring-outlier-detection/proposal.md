# Proposal: rack-wiring-outlier-detection

## Why

A DROID patch can be syntactically valid yet physically nonsensical on the rack. The editor shows circuits as a list, so a wiring error like the MFS-drum case — `E4.4` (left modifier encoder) wired directly to the `M4` fader far right *without a cable* — is invisible in the list but obvious on the rack. Because `droid_tui` already groups components by physical controller and traces cable influence, it is well positioned to surface these geometry outliers that the circuit-list editor cannot.

## What Changes

- **`rack_geometry.json`** — a single source of truth describing rack layout: controller positions, element offsets (B32 4×8 row-wise, vertical mount), and the co-located `L→B` whitelist. Covers the two racks (`R1` horizontal case, `R2` mixed).
- **Geometry feature extractor** — maps each `HwToken` binding to `(src_xy, sink_xy)` in B32-grid units plus `cable_hops`, producing `BindingFeatures`.
- **Track 1 — hard invariant (ships now, no ML):** `graph.rs` topology validation flags a binding when `euclidean_distance > threshold && cable_hops == 0` as a new `TopologyIssue` (Warning), rendered with the existing `graph_edge_error` red token.
- **Track 2 — learned outlier (spike):** a tiny autoencoder on `(src_xy, sink_xy, euclid, adjacent, hops)` trained on good patches, validated against held-out real patches and injected bads. Productized only if it beats Track 1's hard threshold.
- **Data factory** — expands the good corpus (acidified/deepsea/copycat) via MPFS generator and metadroid programmatic assembly, and injects synthetic bads (the MFS-drum pattern and variants).

**Non-goals** (YAGNI):
- No schema validation against the full DROID circuit catalog — only physical-wiring geometry.
- No editor/LSP integration — the detector surfaces in `droid_tui` only.
- No real-hardware connection; the geometry table is a static rack description, not a live read.
- No ML model shipped in the binary unless the Track-2 spike demonstrably beats Track 1.

## Capabilities

### New Capabilities
- `rack-wiring-outlier-detection`: geometry-aware detection of physically implausible HwComponent bindings in a DROID patch — element-precise rack distance vs. cable hops — surfaced as a topology warning in the signal-flow graph view.

### Modified Capabilities
<!-- None. The existing signal-flow-graph topology validation (cable-level n→1) is unchanged; this adds a new, orthogonal physical-geometry check. -->

## Impact

- **Code:** `src/graph.rs` (new validation pass + `TopologyIssue`), `src/patch.rs` (feature extraction hooks), `src/theme.rs` (reuses existing `graph_edge_error` token), `src/ui.rs` (edge highlight — minimal), a new `src/geometry.rs` (geometry table + features).
- **Data:** new `rack_geometry.json` (project-local, machine-specific rack description).
- **Tooling (Track 2 spike, out of the binary):** scripts to expand the corpus via MPFS/metadroid and train/evaluate the autoencoder.
- **Dependencies:** none new in the shipped binary. The spike may use Python (sklearn/torch) offline only.
- **Tests:** geometry table validation, feature extraction, Track-1 invariant (MFS-drum flagged, via-cable not flagged, adjacent not flagged, L→B whitelisted).
