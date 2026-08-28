# Track 2 Data Factory — Regeneration

Offline tooling (out of the shipped binary) that expands the good-patch corpus and emits `BindingFeatures` CSV for the rack-wiring outlier spike.

## Layout

- `corpus/good/` — ~2k good `.ini` patches (14 copied fixtures + ~1986 generated variants)
- `corpus/features.csv` — feature rows (`src_kind,…,cable_hops,is_outlier`)
- `tools/build_features.py` — single deterministic factory (generate + extract + inject + emit)
- `scripts/regenerate.sh` — runner that also checks byte-identical determinism

## Seeds & Determinism

- `SEED=42` used for every `random.Random(SEED + offset)` (generation, per-patch pair sampling, bad-pool choice, cable/HW remaps)
- `TARGET_VARIANTS=2000`, `BAD_COUNT=350`
- Sorted inputs, stable sorting of CSV rows, fixed `euclidean` formatting (`:.6f`), `lineterminator="\n"` → running twice produces byte-identical `corpus/features.csv` (verified via sha256)

If `droid-metapatch` (`metapatch` / `droid_metapatch` importable) is present the generator would use it for MPFS expansion; otherwise it falls back to a pure-Python deterministic permutation + programmatic assembly (no `pip install` without asking). Current repo uses the fallback:

- **Permutation**: shuffle non-`p2b8` sections, remap HW tokens within valid ranges (B 1..32, E 1..4, …) via a per-variant `hw_map`, rename 1–2 `_CABLE`s, tweak 0–1 numeric values
- **Programmatic**: every 3rd variant is built from acid/deepsea/copycat templates (`[lfo]+[sequencer]+[quantizer]` etc.) with random but seeded token/cable choices

## Commands

```bash
# Full rebuild + determinism check
./scripts/regenerate.sh

# Just the factory (no check)
python3 tools/build_features.py

# Verify
wc -l corpus/features.csv          # ~4024 lines (header + ~4023 rows)
head -n 1 corpus/features.csv      # src_kind,sink_kind,param_key,src_x,src_y,sink_x,sink_y,euclidean,manhattan,same_controller,same_rack,adjacent,cable_hops,is_outlier
python3 -c "import csv; rows=list(csv.DictReader(open('corpus/features.csv'))); print(sum(1 for r in rows if r['is_outlier']=='0'), sum(1 for r in rows if r['is_outlier']=='1'))"
# Determinism
sha256sum corpus/features.csv
python3 tools/build_features.py && sha256sum corpus/features.csv  # identical
```

## Geometry replica

`tools/build_features.py` re-implements `src/geometry.rs`:

- `resolve()` → `abs_x/slot.x+off`, `abs_y/rack.y+off` (matrix/stack/singleton)
- `distance`/`adjacent`/`same_controller`/`same_rack`
- `compute_cable_hops()` via `circuit_outputs` + `section_consumes_cable` BFS (matches Rust)
- `BindingFeatures` fields mirror the Rust struct; Python uses same `token_kind_u8` mapping (B0 L1 P2 O3 I4 E5 S6 G7 R/M8)

## Runtime

~2–4 s on a laptop for 2000 patches (fast path for >10 KB fixtures). No network.

## Guardrails

New directories only (`corpus/`, `tools/`, `scripts/`); nothing under `src/` or `Cargo.toml` is touched.
