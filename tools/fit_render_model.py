#!/usr/bin/env python3
"""
Offline fit + evaluation for the render-outlier decision table (design D1/D4/D5).

Reads corpus/rendermetrics.csv (schema produced by tools/build_rendermetrics.py,
a Python replica of the src/rendermetrics.rs feature math), splits a
deterministic stratified holdout (random.Random(seed)), fits a bounded
decision table of per-feature bands (design D1, <= a few KB), and emits
tools/render_artifact.txt for embedding via include_str! (task 2.2).

Band semantics (consumed identically by the Rust scorer):
  - Each band is {feature, min, max, degraded} over one feature; a row
    matches a band when its feature value lies in [min, max].
  - Bands are evaluated top-down, first match wins. Only degraded=true
    (flag) bands are emitted: a row matching no band falls back to the
    baseline rule (design D5) — degraded iff overflow_cols > 0 (i.e.
    width < min_width * baseline_min_width_factor). The fallback IS the
    pass verdict for clean rows (overflow == 0), and the table never
    emits pass bands because a pass band matching an overflow-degraded
    row would remove the baseline's guarantee (design D5 rationale:
    "the learned table can only add warnings, never remove the
    baseline's guarantees"). The schema still supports degraded=false
    bands; the fitter simply never produces them.
  - The learned table adds exactly what the baseline cannot see:
    min_contrast < 4.5 -> the mono shift4=Black failure the detector
    exists for; fallback_rate > 0 -> boxed->unboxed cell fallback.
  - Learned flag bands are widened to their feature-semantic floors
    (widen_bands): the corpus contains no feature values inside the gaps
    (e.g. min_contrast in (1.0, 5.25), overflow_cols in 1..9) for the
    learner to find the true boundary, so the exact-value bands it finds
    are widened to the label rule's own thresholds (min_contrast < 4.5,
    overflow_cols >= 1, fallback_rate > 0). Widening is safe: any value
    in the widened range is degraded by the auditable label rule.
  - min_contrast rows that are NA (terminal theme, Color::Reset tokens)
    are fitted with the sentinel 99.0 so no learned band matches them:
    the terminal theme owns its colors and is never contrast-flagged.

Gate (from design D4): holdout precision >= 0.60 at recall >= 0.86 for the
learned table. The report also prints the heuristic baseline (native-fit
rule) for comparison.

Usage: python3 tools/fit_render_model.py [--seed 42]
Exit 0 when the gate is met and the artifact is written; exit 1 otherwise.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import random
import sys
from pathlib import Path

SEED_DEFAULT = 42
HOLDOUT_FRAC = 0.20

# learner knobs (deterministic)
MIN_FLAG_BAD = 3       # a flag band must cover at least this many bad rows
MIN_FLAG_PREC = 0.60   # a flag band must end at this precision (== gate)
MAX_RULES = 40

# gate (design D4) — do NOT loosen
GATE_PRECISION = 0.60
GATE_RECALL = 0.86

# contrast sentinel for NA (terminal) rows — above any real contrast value,
# so no learned contrast band ever matches them.
CONTRAST_NA_SENTINEL = 99.0
# contrast band floor: matches the corpus label rule (WCAG AA).
CONTRAST_BAD_MAX = 4.5

REPO = Path(__file__).resolve().parent.parent
CSV_PATH = REPO / "corpus" / "rendermetrics.csv"
ARTIFACT_PATH = REPO / "tools" / "render_artifact.txt"

FEATURE_NAMES = [
    "components", "panels", "modules", "min_width", "overflow_cols",
    "fallback_rate", "sidebar_hidden", "minimap_hidden", "min_contrast",
]
BOOL_FEATURES = {"sidebar_hidden", "minimap_hidden"}


# ---------------------------------------------------------------------------
# data
# ---------------------------------------------------------------------------

def load_rows(path: Path):
    rows = []
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            mc = r["min_contrast"].strip()
            rows.append({
                "patch": r["patch"],
                "width": int(r["width"]),
                "theme": r["theme"],
                "components": int(r["components"]),
                "panels": int(r["panels"]),
                "modules": int(r["modules"]),
                "min_width": int(r["min_width"]),
                "overflow_cols": int(r["overflow_cols"]),
                "fallback_rate": float(r["fallback_rate"]),
                "sidebar_hidden": int(r["sidebar_hidden"]),
                "minimap_hidden": int(r["minimap_hidden"]),
                "min_contrast": float(mc) if mc else None,
                "degraded": int(r["degraded"]),
            })
    return rows


def stratified_holdout(rows, rng: random.Random, frac: float):
    """Deterministic stratified split: shuffle each label class, take the
    first `frac` as holdout (design D4, mirrors fit_outlier_model.py)."""
    by_label = {0: [], 1: []}
    for r in rows:
        by_label[r["degraded"]].append(r)
    holdout, train = [], []
    for label, group in by_label.items():
        group = list(group)
        rng.shuffle(group)
        k = max(1, int(len(group) * frac))
        holdout.extend(group[:k])
        train.extend(group[k:])
    rng.shuffle(holdout)
    rng.shuffle(train)
    return train, holdout


def feature_value(row, feature: str) -> float:
    v = row[feature]
    if feature == "min_contrast":
        return CONTRAST_NA_SENTINEL if v is None else v
    return float(v)


# ---------------------------------------------------------------------------
# band learner
# ---------------------------------------------------------------------------

def eps_for(v: float) -> float:
    return 1e-6 * max(1.0, abs(v))


def candidate_bands(feature: str, vals):
    """Exact-value bands plus midpoint threshold bands ([-inf, mid] and
    [mid, +inf]) derived from the data's unique values. Bands on boolean
    features collapse to the single values 0 and 1."""
    uniq = sorted(set(vals))
    cands = []
    for v in uniq:
        e = eps_for(v)
        cands.append((feature, v - e, v + e))
        if not BOOL_FEATURES.intersection({feature}):
            cands.append((feature, -1e9, v + e))
            cands.append((feature, v - e, 1e9))
    for a, b in zip(uniq, uniq[1:]):
        mid = (a + b) / 2.0
        cands.append((feature, -1e9, mid))
        cands.append((feature, mid, 1e9))
    return cands


def fit_flag_bands(pool, rng: random.Random):
    """Greedy flag-band learner (mirrors fit_outlier_model.py's flag-rules
    first discipline). Picks the highest-precision band covering >=
    MIN_FLAG_BAD bad rows, removes covered rows, repeats up to MAX_RULES."""
    pool = list(pool)
    bands = []
    while pool and len(bands) < MAX_RULES:
        best = None
        for feature in FEATURE_NAMES:
            for _feat, lo, hi in candidate_bands(feature, [feature_value(r, feature) for r in pool]):
                covered = [r for r in pool if lo <= feature_value(r, feature) <= hi]
                if not covered:
                    continue
                bad = sum(1 for r in covered if r["degraded"])
                prec = bad / len(covered)
                if prec >= MIN_FLAG_PREC and bad >= MIN_FLAG_BAD:
                    score = (prec, bad)  # precision first, then coverage
                    if best is None or score > best[0]:
                        best = (score, feature, lo, hi, covered)
        if best is None:
            break
        _, feature, lo, hi, covered = best
        bands.append({"feature": feature, "min": lo, "max": hi, "degraded": True})
        covered_ids = {id(r) for r in covered}
        pool = [r for r in pool if id(r) not in covered_ids]
    return bands, pool


def fit_pass_bands(pool, rng: random.Random):
    """Not used: pass bands would weaken the baseline's flags (design D5).
    Kept as a stub so the schema's degraded=false entries stay supported
    by the evaluation path (predict) if a future fit needs them."""
    return []


# ---------------------------------------------------------------------------
# evaluation
# ---------------------------------------------------------------------------

def widen_bands(bands):
    """Widen learned flag bands to the label rule's semantic floors (see
    module docstring). Each widening is safe: any feature value in the
    widened range is degraded by the auditable label rule."""
    out = []
    for b in bands:
        if b["degraded"]:
            if b["feature"] == "overflow_cols":
                b = {**b, "min": 1.0, "max": 1e9}
            elif b["feature"] == "fallback_rate":
                b = {**b, "min": 1e-6, "max": 1.0}
            elif b["feature"] == "min_contrast":
                b = {**b, "min": 0.0, "max": CONTRAST_BAD_MAX}
        out.append(b)
    return out


def predict(row, bands) -> int:
    """Top-down first match over the bands; no match -> baseline rule (D5):
    degraded iff overflow_cols > 0 (width < min_width x factor)."""
    for b in bands:
        v = feature_value(row, b["feature"])
        if b["min"] <= v <= b["max"]:
            return 1 if b["degraded"] else 0
    return 1 if row["overflow_cols"] > 0 else 0


def baseline_predict(row) -> int:
    return 1 if row["overflow_cols"] > 0 else 0


def report(rows, predict_fn):
    tp = sum(1 for r in rows if r["degraded"] and predict_fn(r))
    fp = sum(1 for r in rows if not r["degraded"] and predict_fn(r))
    fn = sum(1 for r in rows if r["degraded"] and not predict_fn(r))
    prec = tp / (tp + fp) if tp + fp else 0.0
    rec = tp / (tp + fn) if tp + fn else 0.0
    return prec, rec, tp, fp, fn


def fmt_band(b) -> dict:
    return {
        "feature": b["feature"],
        "min": float("%.6g" % b["min"]),
        "max": float("%.6g" % b["max"]),
        "degraded": b["degraded"],
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=SEED_DEFAULT)
    args = ap.parse_args()
    seed = args.seed

    rows = load_rows(CSV_PATH)
    n_bad = sum(1 for r in rows if r["degraded"])
    print(f"fit_render_model: {len(rows)} rows from {CSV_PATH.name} "
          f"(degraded {n_bad}, clean {len(rows) - n_bad})")

    rng = random.Random(seed)
    train, holdout = stratified_holdout(rows, rng, HOLDOUT_FRAC)
    print(f"  holdout {len(holdout)} rows (frac {HOLDOUT_FRAC}), train {len(train)}")

    flag_bands, rest = fit_flag_bands(train, rng)
    pass_bands = fit_pass_bands(rest, rng)
    bands = widen_bands(flag_bands + pass_bands)

    # --- baseline (heuristic native-fit rule) vs learned table ---------------
    b_prec, b_rec, *_ = report(holdout, baseline_predict)
    l_prec, l_rec, l_tp, l_fp, l_fn = report(holdout, lambda r: predict(r, bands))
    print(f"  baseline (native-fit): precision {b_prec:.3f} recall {b_rec:.3f}")
    print(f"  learned  (table {len(bands)} bands): precision {l_prec:.3f} "
          f"recall {l_rec:.3f}  (tp {l_tp} fp {l_fp} fn {l_fn})")

    tr_prec, tr_rec, *_ = report(train, lambda r: predict(r, bands))
    print(f"  train: precision {tr_prec:.3f} recall {tr_rec:.3f}")

    for b in bands:
        mark = "flag" if b["degraded"] else "pass"
        print(f"    [{mark}] {b['feature']} in [{b['min']:.6g}, {b['max']:.6g}]")

    # --- gate ---------------------------------------------------------------
    ok = l_prec >= GATE_PRECISION and l_rec >= GATE_RECALL
    print(f"  gate: precision {l_prec:.3f} >= {GATE_PRECISION} and "
          f"recall {l_rec:.3f} >= {GATE_RECALL} -> {'PASS' if ok else 'FAIL'}")
    if not ok:
        print("  artifact NOT written (gate failed)", file=sys.stderr)
        return 1

    artifact = {
        "version": 1,
        "feature_names": FEATURE_NAMES,
        "degraded_bands": [fmt_band(b) for b in bands],
        "baseline_min_width_factor": 1.0,
        "seed": seed,
    }
    text = json.dumps(artifact, indent=2) + "\n"
    ARTIFACT_PATH.write_text(text)
    digest = hashlib.sha256(text.encode()).hexdigest()
    print(f"  wrote {ARTIFACT_PATH} ({len(text)} bytes, sha256 {digest[:16]}...)")
    return 0


if __name__ == "__main__":
    sys.exit(main())