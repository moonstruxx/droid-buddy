#!/usr/bin/env python3
"""
Offline fit + evaluation for the wiring-outlier decision table (design D1/D2).

Reads corpus/features.csv (schema produced by tools/build_features.py, a Python
replica of the src/geometry.rs feature math), splits a deterministic holdout
(seeded random.Random, stratified by label), fits a bounded decision table with
a deterministic greedy rule learner (flag rules first, then pass rules), and
emits tools/outlier_artifact.txt for embedding via include_str! (schema.rs
precedent).

Runtime semantics mirrored here: the learned scorer is consulted only after the
invariant guards pass (adjacent / co-located euclidean<1e-6 / via-cable rows are
never scored, design D5). Rows matching no table row fall back to the threshold
rule `euclidean > 8.0 && cable_hops == 0`, which preserves today's behavior
(design D1) — the table only overrides the rule where it can do better.

Usage: python3 tools/fit_outlier_model.py [--seed 42]
Exit 0 when the fitted table meets the gate on the holdout and the artifact is
written; exit 1 otherwise (gate precision >= 0.60 at recall >= 0.86).
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import random
import sys
from pathlib import Path

SEED_DEFAULT = 42
HOLDOUT_FRAC = 0.20

# learner knobs (deterministic)
MIN_FLAG_BAD = 3        # a flag rule must cover at least this many bad rows
BASE_SEED_PREC = 0.20   # base condition must beat this to seed refinement
MIN_FLAG_PREC = 0.50    # a refined flag rule must end at this precision
MIN_PASS_GOOD = 1       # a pass rule must cover at least this many good rows
MAX_RULES = 40

# gate (from tasks 1.2 / design D2) — do NOT loosen
GATE_PRECISION = 0.60
GATE_RECALL = 0.86

REPO = Path(__file__).resolve().parent.parent
CSV_PATH = REPO / "corpus" / "features.csv"
ARTIFACT_PATH = REPO / "tools" / "outlier_artifact.txt"

KINDS = [0, 1, 2, 3, 4, 5, 6, 7, 8]
KIND_LETTER = {0: "B", 1: "L", 2: "P", 3: "O", 4: "I", 5: "E", 6: "S", 7: "G", 8: "M"}


# ---------------------------------------------------------------------------
# data
# ---------------------------------------------------------------------------

def load_rows(path: Path):
    rows = []
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            rows.append({
                "src_kind": int(r["src_kind"]),
                "sink_kind": int(r["sink_kind"]),
                "euclidean": float(r["euclidean"]),
                "manhattan": float(r["manhattan"]),
                "same_controller": int(r["same_controller"]),
                "same_rack": int(r["same_rack"]),
                "adjacent": int(r["adjacent"]),
                "cable_hops": int(r["cable_hops"]),
                "is_outlier": int(r["is_outlier"]),
            })
    return rows


def scorer_visible(rows):
    """Rows the runtime scorer can see. Invariant guards (design D5) already
    rejected adjacent / co-located / via-cable bindings before the scorer runs,
    so those rows are neither fitted nor evaluated."""
    out = []
    for r in rows:
        if r["adjacent"] == 1:
            continue
        if r["euclidean"] < 1e-6:
            continue
        if r["cable_hops"] != 0:
            continue
        out.append(r)
    return out


def split_holdout(rows, seed):
    """Stratified, seeded holdout split: same fraction of bads and goods."""
    rng = random.Random(seed)
    bad_idx = [i for i, r in enumerate(rows) if r["is_outlier"]]
    good_idx = [i for i, r in enumerate(rows) if not r["is_outlier"]]
    rng.shuffle(bad_idx)
    rng.shuffle(good_idx)
    n_bad_ho = max(1, int(round(len(bad_idx) * HOLDOUT_FRAC)))
    n_good_ho = int(round(len(good_idx) * HOLDOUT_FRAC))
    ho = set(bad_idx[:n_bad_ho]) | set(good_idx[:n_good_ho])
    train = [r for i, r in enumerate(rows) if i not in ho]
    holdout = [r for i, r in enumerate(rows) if i in ho]
    return train, holdout


# ---------------------------------------------------------------------------
# rule conditions
# ---------------------------------------------------------------------------
# cond = (field, op, value); ops: '==', 'box' (lo,hi), '>=', '<='


def cond_match(r, cond):
    field, op, value = cond
    if op == "==":
        return r[field] == value
    if op == "box":
        lo, hi = value
        return lo <= r[field] <= hi
    if op == ">=":
        return r[field] >= value
    if op == "<=":
        return r[field] <= value
    raise ValueError(op)


def pair_conds(sk, kk):
    return (("src_kind", "==", sk), ("sink_kind", "==", kk))


def row_matches(r, rule_conds):
    return all(cond_match(r, c) for c in rule_conds)


def split_labels(matched):
    b = sum(1 for r in matched if r["is_outlier"])
    return b, len(matched) - b


def candidate_conds(pool):
    """Deterministic condition space: boxes + thresholds around the exact
    feature values present in the pool, plus the categorical splits."""
    conds = []
    eucl_vals = sorted({round(r["euclidean"], 4) for r in pool})
    manh_vals = sorted({float(r["manhattan"]) for r in pool})
    for v in eucl_vals:
        conds.append(("euclidean", "box", (v - 0.05, v + 0.05)))
        conds.append(("euclidean", ">=", v))
        conds.append(("euclidean", "<=", v))
    for v in manh_vals:
        conds.append(("manhattan", "box", (v - 0.05, v + 0.05)))
        conds.append(("manhattan", ">=", v))
        conds.append(("manhattan", "<=", v))
    for v in (0, 1):
        conds.append(("same_controller", "==", v))
        conds.append(("same_rack", "==", v))
    return conds


# ---------------------------------------------------------------------------
# greedy learner
# ---------------------------------------------------------------------------

def learn_flags(pool):
    """Flag rules: clean kind-pair rules first, then box rules refined inside
    the impure pairs. Returns (flag_rules, remaining_pool)."""
    flags = []
    pool = list(pool)

    # Phase A: kind pairs with zero good rows anywhere are perfect flag rules.
    for sk in KINDS:
        for kk in KINDS:
            pc = pair_conds(sk, kk)
            matched = [r for r in pool if row_matches(r, pc)]
            b, g = split_labels(matched)
            if b >= MIN_FLAG_BAD and g == 0:
                flags.append(pc)
                pool = [r for r in pool if not row_matches(r, pc)]

    # Phase B: for each pair that still holds bads, mine flag boxes greedily.
    for sk in KINDS:
        for kk in KINDS:
            pc = pair_conds(sk, kk)
            pair_rows = [r for r in pool if row_matches(r, pc)]
            pb, _ = split_labels(pair_rows)
            if pb < MIN_FLAG_BAD:
                continue
            remaining = list(pair_rows)
            while len(flags) < MAX_RULES:
                best = None
                for c in candidate_conds(remaining):
                    matched = [r for r in remaining if cond_match(r, c)]
                    b, g = split_labels(matched)
                    if b < MIN_FLAG_BAD:
                        continue
                    prec = b / (b + g)
                    if prec < BASE_SEED_PREC:
                        continue
                    score = (prec, b)
                    if best is None or score > best[0]:
                        best = (score, c, b, g)
                if best is None:
                    break
                _, c, b, g = best
                rule = [c]
                # refine: conjoin conditions while precision climbs
                while len(rule) < 4:
                    cur_prec = b / (b + g)
                    improved = None
                    for c2 in candidate_conds(remaining):
                        if c2 in rule:
                            continue
                        matched = [r for r in remaining
                                   if all(cond_match(r, cc) for cc in rule + [c2])]
                        bb, gg = split_labels(matched)
                        if bb < MIN_FLAG_BAD:
                            continue
                        p = bb / (bb + gg)
                        if p > cur_prec + 1e-9 and (improved is None or p > improved[1]):
                            improved = (c2, p, bb, gg)
                    if improved is None:
                        break
                    c2, p, bb, gg = improved
                    rule.append(c2)
                    b, g = bb, gg
                full = pc + tuple(rule)
                if b / (b + g) < MIN_FLAG_PREC:
                    break  # best refinement still too impure; pair is not separable
                flags.append(full)
                remaining = [r for r in remaining if not row_matches(r, full)]
                if sum(r["is_outlier"] for r in remaining) < MIN_FLAG_BAD:
                    break

    pool = [r for r in pool if not any(row_matches(r, fl) for fl in flags)]
    return flags, pool


def learn_passes(pool):
    """Pass rules: clean regions (zero remaining bads) with the largest good
    coverage first, so the fallback rule is not left firing on them."""
    passes_rules = []
    pool = list(pool)
    while len(passes_rules) < MAX_RULES:
        cands = []
        for sk in KINDS:
            for kk in KINDS:
                pc = pair_conds(sk, kk)
                cands.append(pc)
                cands.append(pc + (("same_controller", "==", 0),))
                cands.append(pc + (("same_controller", "==", 1),))
                eucl_vals = sorted({round(r["euclidean"], 4) for r in pool
                                    if r["src_kind"] == sk and r["sink_kind"] == kk})
                for v in eucl_vals:
                    cands.append(pc + (("euclidean", ">=", v),))
                    cands.append(pc + (("euclidean", "<=", v),))
        # dedupe preserving deterministic order
        seen = set()
        uniq = []
        for c in cands:
            key = tuple(sorted(c))
            if key not in seen:
                seen.add(key)
                uniq.append(c)
        best = None
        for rule in uniq:
            matched = [r for r in pool if row_matches(r, rule)]
            g = sum(1 for r in matched if not r["is_outlier"])
            b = len(matched) - g
            if b == 0 and g >= MIN_PASS_GOOD:
                score = (g, len(rule))  # largest coverage; tie -> simplest rule
                if best is None or score > best[0]:
                    best = (score, tuple(rule), g)
        if best is None:
            break
        _, rule, _ = best
        passes_rules.append(rule)
        pool = [r for r in pool if not row_matches(r, rule)]
    return passes_rules, pool


# ---------------------------------------------------------------------------
# evaluation
# ---------------------------------------------------------------------------

def baseline_rule(r):
    return r["euclidean"] > 8.0 and r["cable_hops"] == 0


def evaluate(rows, table):
    """rows: scorer-visible rows. table: [(rule_conds, verdict)] ordered.
    First match wins; no match -> fallback threshold rule."""
    tp = fp = fn = 0
    fallback_hits = 0
    fallback_flags = 0
    for r in rows:
        verdict = None
        for rule, v in table:
            if row_matches(r, rule):
                verdict = v
                break
        if verdict is None:
            fallback_hits += 1
            verdict = "flag" if baseline_rule(r) else "pass"
            if verdict == "flag":
                fallback_flags += 1
        if verdict == "flag":
            if r["is_outlier"]:
                tp += 1
            else:
                fp += 1
        else:
            if r["is_outlier"]:
                fn += 1
    precision = tp / (tp + fp) if (tp + fp) else 0.0
    recall = tp / (tp + fn) if (tp + fn) else 0.0
    return {"tp": tp, "fp": fp, "fn": fn,
            "precision": precision, "recall": recall,
            "fallback_hits": fallback_hits, "fallback_flags": fallback_flags}


def evaluate_baseline(rows):
    tp = fp = fn = 0
    for r in rows:
        pred = baseline_rule(r)
        if pred and r["is_outlier"]:
            tp += 1
        elif pred and not r["is_outlier"]:
            fp += 1
        elif not pred and r["is_outlier"]:
            fn += 1
    precision = tp / (tp + fp) if (tp + fp) else 0.0
    recall = tp / (tp + fn) if (tp + fn) else 0.0
    return {"tp": tp, "fp": fp, "fn": fn, "precision": precision, "recall": recall}


# ---------------------------------------------------------------------------
# artifact
# ---------------------------------------------------------------------------

def rule_to_row(rule, verdict):
    """Canonical 8-column table row from a rule's conditions."""
    lo_e = hi_e = lo_m = hi_m = None
    sc = "*"
    sk = kk = None
    for field, op, value in rule:
        if field == "euclidean":
            if op == "box":
                lo, hi = value
                lo_e = lo if lo_e is None else max(lo_e, lo)
                hi_e = hi if hi_e is None else min(hi_e, hi)
            elif op == ">=":
                lo_e = value if lo_e is None else max(lo_e, value)
            elif op == "<=":
                hi_e = value if hi_e is None else min(hi_e, value)
        elif field == "manhattan":
            if op == "box":
                lo, hi = value
                lo_m = lo if lo_m is None else max(lo_m, lo)
                hi_m = hi if hi_m is None else min(hi_m, hi)
            elif op == ">=":
                lo_m = value if lo_m is None else max(lo_m, value)
            elif op == "<=":
                hi_m = value if hi_m is None else min(hi_m, value)
        elif field == "same_controller":
            sc = value
        elif field == "src_kind":
            sk = value
        elif field == "sink_kind":
            kk = value
    lo_e = 0.0 if lo_e is None else lo_e
    hi_e = 1e9 if hi_e is None else hi_e
    lo_m = 0.0 if lo_m is None else lo_m
    hi_m = 1e9 if hi_m is None else hi_m
    src = "*" if sk is None else str(sk)
    sink = "*" if kk is None else str(kk)
    return f"{lo_e:.6f} {hi_e:.6f} {lo_m:.6f} {hi_m:.6f} {sc} {src} {sink} {verdict}"


def write_artifact(table, seed, row_count, sha):
    header = "\n".join([
        "# droid_tui wiring-outlier decision table (design D1)",
        f"# Fitted by tools/fit_outlier_model.py --seed {seed} on corpus/features.csv",
        "#",
        "# Columns (whitespace-separated, one rule per line, '#' comments allowed):",
        "#   min_euclidean max_euclidean min_manhattan max_manhattan same_controller src_kind sink_kind verdict",
        "# Ranges are inclusive; * means 'any' for the categorical columns.",
        "# Verdict: flag | pass. Rules are evaluated top to bottom; first match wins.",
        "# Rows matching no rule fall back to the threshold rule:",
        "#   euclidean > 8.0 && cable_hops == 0  (current behavior, preserved)",
        "# The scorer is consulted only after the invariant guards pass",
        "# (adjacent / co-located / via-cable bindings never reach the table).",
        "# Artifact sha256: " + sha,
        "",
    ])
    body = "\n".join(rule_to_row(rule, verdict) for rule, verdict in table)
    ARTIFACT_PATH.write_text(header + body + "\n")


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=SEED_DEFAULT,
                        help="holdout split seed (default %(default)s)")
    args = parser.parse_args(argv)

    rows = load_rows(CSV_PATH)
    vis = scorer_visible(rows)
    train, holdout = split_holdout(rows, args.seed)
    vis_train = scorer_visible(train)
    vis_holdout = scorer_visible(holdout)

    flags, pool_after_flags = learn_flags(vis_train)
    passes_rules, _ = learn_passes(pool_after_flags)
    table = [(fl, "flag") for fl in flags] + [(ps, "pass") for ps in passes_rules]

    base = evaluate_baseline(vis_holdout)
    fit = evaluate(vis_holdout, table)

    gate_ok = fit["precision"] >= GATE_PRECISION and fit["recall"] >= GATE_RECALL

    # artifact (stable bytes -> hash printed in the header too)
    n_bad_full = sum(r["is_outlier"] for r in rows)
    n_good_full = len(rows) - n_bad_full
    n_vis = len(vis)
    n_vis_bad = sum(r["is_outlier"] for r in vis)
    n_ho_bad = sum(r["is_outlier"] for r in holdout)
    n_ho_good = len(holdout) - n_ho_bad
    n_vis_ho = len(vis_holdout)
    n_vis_ho_bad = sum(r["is_outlier"] for r in vis_holdout)

    sha = ""
    if gate_ok:
        write_artifact(table, args.seed, len(table), sha)  # placeholder hash
        sha = hashlib.sha256(ARTIFACT_PATH.read_bytes()).hexdigest()
        # rewrite with the real hash (deterministic: same bytes -> same hash)
        write_artifact(table, args.seed, len(table), sha)
        sha = hashlib.sha256(ARTIFACT_PATH.read_bytes()).hexdigest()
        assert sha == hashlib.sha256(ARTIFACT_PATH.read_bytes()).hexdigest()

    print(f"corpus: {len(rows)} rows ({n_good_full} good / {n_bad_full} bad); "
          f"scorer-visible {n_vis} ({n_vis - n_vis_bad} good / {n_vis_bad} bad)")
    print(f"holdout {len(holdout)} rows: {n_ho_good} good / {n_ho_bad} bad "
          f"(scorer-visible {n_vis_ho}: {n_vis_ho - n_vis_ho_bad} good / {n_vis_ho_bad} bad)")
    print(f"baseline (euclidean > 8.0 && cable_hops == 0): "
          f"precision {base['precision']:.3f} recall {base['recall']:.3f} "
          f"(tp={base['tp']} fp={base['fp']} fn={base['fn']})")
    print(f"fitted table (+ fallback): precision {fit['precision']:.3f} recall {fit['recall']:.3f} "
          f"(tp={fit['tp']} fp={fit['fp']} fn={fit['fn']})")
    print(f"gate: precision >= {GATE_PRECISION} and recall >= {GATE_RECALL}  ->  "
          f"{'PASS' if gate_ok else 'FAIL'}")
    print(f"fallback coverage: {fit['fallback_hits']} of {n_vis_ho} scorer-visible holdout rows "
          f"match no table row ({fit['fallback_flags']} of them flagged by fallback)")
    print(f"table: {len(flags)} flag rules + {len(passes_rules)} pass rules = {len(table)} rows")
    if gate_ok:
        size = ARTIFACT_PATH.stat().st_size
        print(f"artifact: {ARTIFACT_PATH.relative_to(REPO)}  {len(table)} rows  "
              f"{size} bytes  sha256 {sha}")
    else:
        print("artifact: NOT WRITTEN (gate failed)")
    return 0 if gate_ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))