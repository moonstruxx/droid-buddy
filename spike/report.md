# Spike 3.2 — Tiny autoencoder vs hard wiring threshold

Date: 2026-08-28
Seed: 42
Corpus: `corpus/features.csv` (3673 good + 350 synthetic bad, 4023 rows), 2000 patches in `corpus/good/`

## Baseline (Track 1 hard invariant)
- Rule: `euclidean > 8.0 && cable_hops == 0` flagged as `TopologyIssue(Warning)` (`src/graph.rs:WIRING_DISTANCE_THRESHOLD`)
- Held-out test (15% good ≈551, all 350 bad):
  - TPR 0.857 (300/350), FPR 0.450 (248/551), Acc 0.669
- Interpretable, zero training, already ships and passes 388 tests.

## Spike model
- Features (12 numeric): src_kind, sink_kind, src_x/y, sink_x/y, euclidean, manhattan, same_controller, same_rack, adjacent, cable_hops (param_key dropped — factory emits "0" for all rows in fallback mode, not informative)
- Split: 85% good train (≈3122), 15% good test (≈551) + 350 bad
- Normalization: mean/std from train
- Surrogate autoencoder: PCA 12→4→12 (SVD, Vk = top-4 eigenvectors) — proxy for tiny 32→8→32 autoencoder; reconstruction error = MSE per row; threshold = percentile of train errors.
- No pip deps beyond numpy (offline, deterministic).

## Results
| threshold | TPR (bad recalled) | FPR (good flagged) | Acc |
|-----------|-------------------:|-------------------:|-----:|
| 90 pct (0.467) | 0.857 | 0.113 | **0.876** |
| 95 pct (1.169) | 0.149 | 0.067 | 0.628 |
| 97 pct (2.551) | 0.000 | 0.042 | 0.586 |

- At 90-pct threshold the PCA surrogate **matches hard recall (0.857) with 4× lower false positives (0.11 vs 0.45) and higher accuracy (0.876 vs 0.669)**.
- At 95-pct (default) it under-recalls badly (0.15).
- Mean recon error: good 0.31 vs bad 0.97 (bad higher as intended).

## Interpretation
- With threshold tuning the soft model can beat the hard gate on this synthetic corpus, but the win is entirely threshold-dependent and not robust (small shift from 90→95 pct collapses TPR).
- Feature set is weak (param_key uninformative in fallback generator, distances dominate) so the model is essentially re-learning `euclidean` + `cable_hops`.
- Hard gate is already conservative and interpretable; soft model adds training, calibration, and drift risk for marginal gain on synthetic data.

## Decision (Task 3.3)
**Keep Track 1 only (YAGNI).** Do not productize the autoencoder now.

Follow-up bead only if:
- Real (non-synthetic) bad labels exist, param_key/cable features become informative, and we re-run with a 32-dim embedding + proper autoencoder; or
- We need UI outliers beyond wiring (topology histogram / visual render) where distance alone fails.

Repro: `python3 tools/build_features.py && python3 spike/eval.py` (eval script below). Deterministic (SEED=42).
