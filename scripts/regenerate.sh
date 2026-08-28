#!/usr/bin/env bash
# regenerate.sh — rebuild Track 2 corpus and features CSV deterministically
# Usage: ./scripts/regenerate.sh
# Requires: python3 (no extra pip packages), rack_geometry.json at repo root
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
echo "Seeds: SEED=42, TARGET=2000 variants, BAD_COUNT=350"
echo "Method: deterministic permutation + programmatic assembly (fallback when droid-metapatch not installed)"
echo "Checking droid-metapatch availability..."
if python3 -c "import importlib.util; exit(0 if importlib.util.find_spec('metapatch') or importlib.util.find_spec('droid_metapatch') else 1)" 2>/dev/null; then
  echo "  droid-metapatch found — would use MPFS generator (not exercised in fallback)"
else
  echo "  droid-metapatch NOT found — using fallback deterministic permutation (no pip install)"
fi
echo "Running tools/build_features.py ..."
python3 tools/build_features.py
echo ""
echo "Verifying determinism (second run byte-identical)..."
sha1=$(sha256sum corpus/features.csv | awk '{print $1}')
python3 tools/build_features.py >/dev/null 2>&1
sha2=$(sha256sum corpus/features.csv | awk '{print $1}')
if [ "$sha1" = "$sha2" ]; then
  echo "  OK byte-identical: $sha1"
else
  echo "  FAIL hashes differ: $sha1 != $sha2"
  exit 1
fi
echo ""
wc -l corpus/features.csv
head -n 1 corpus/features.csv
echo "Done. Corpus: $(ls -1 corpus/good/*.ini | wc -l) patches in corpus/good/"
