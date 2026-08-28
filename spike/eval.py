#!/usr/bin/env python3
"""Spike eval: PCA 12->4->12 vs hard threshold. Deterministic SEED=42."""
import csv, random
import numpy as np
from pathlib import Path

rows=list(csv.DictReader(open("corpus/features.csv")))
def row_to_vec(r):
    return [int(r['src_kind']), int(r['sink_kind']),
            float(r['src_x']), float(r['src_y']), float(r['sink_x']), float(r['sink_y']),
            float(r['euclidean']), float(r['manhattan']),
            int(r['same_controller']), int(r['same_rack']), int(r['adjacent']), int(r['cable_hops'])]
random.seed(42)
good=[r for r in rows if r['is_outlier']=='0']
bad=[r for r in rows if r['is_outlier']=='1']
random.shuffle(good)
n_train=int(0.85*len(good))
train, test_good = good[:n_train], good[n_train:]

X_train=np.array([row_to_vec(r) for r in train], float)
X_test_good=np.array([row_to_vec(r) for r in test_good], float)
X_test_bad=np.array([row_to_vec(r) for r in bad], float)
mean=X_train.mean(0); std=X_train.std(0); std[std<1e-6]=1.0
Xn_train=(X_train-mean)/std; Xn_test_good=(X_test_good-mean)/std; Xn_test_bad=(X_test_bad-mean)/std
from numpy.linalg import svd
U,S,Vt=svd(Xn_train, full_matrices=False)
k=4; Vk=Vt[:k].T
def recon_error(Xn): return ((Xn - (Xn@Vk)@Vk.T)**2).mean(1)
err_train=recon_error(Xn_train); err_good=recon_error(Xn_test_good); err_bad=recon_error(Xn_test_bad)
def metrics(ag, ab):
    tp=(ab).sum(); fn=len(ab)-tp; fp=(ag).sum(); tn=len(ag)-fp
    return dict(tp=int(tp), fn=int(fn), fp=int(fp), tn=int(tn), tpr=tp/(tp+fn), fpr=fp/(fp+tn), acc=(tp+tn)/(tp+tn+fp+fn))
for pct in [90,95,97,99]:
    th=np.percentile(err_train,pct); m=metrics(err_good>th, err_bad>th)
    print(f"pct {pct} th {th:.4f} -> tpr {m['tpr']:.3f} fpr {m['fpr']:.3f} acc {m['acc']:.3f} {m}")
hard_good=np.array([float(r['euclidean'])>8.0 and int(r['cable_hops'])==0 for r in test_good])
hard_bad=np.array([float(r['euclidean'])>8.0 and int(r['cable_hops'])==0 for r in bad])
print("hard", metrics(hard_good, hard_bad))
