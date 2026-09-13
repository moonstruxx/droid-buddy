#!/usr/bin/env bash
# profile-gate.sh — profile one perf-gate phase under perf record and render a flamegraph
# Usage: ./scripts/profile-gate.sh <phase>
#   <phase>  cargo test-name filter into tests/perf_gate.rs (e.g. phase_parse_render)
# Always writes .opencode/.tmp/profiling/<phase>.perf.data; renders <phase>.svg when
# stackcollapse-perf.pl and flamegraph.pl are both on PATH, else keeps the raw
# perf.data (still exits 0).
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <phase>" >&2
  echo "  <phase>  cargo test-name filter into tests/perf_gate.rs" >&2
  exit 1
fi
phase="$1"
case "$phase" in
  */*) echo "Error: phase must not contain '/'" >&2; exit 1 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

out_dir="$ROOT/.opencode/.tmp/profiling"
mkdir -p "$out_dir"

perf_data="$out_dir/$phase.perf.data"
svg="$out_dir/$phase.svg"

echo "Profiling phase '$phase' (release, locked) under perf record..."
perf record -o "$perf_data" -- \
  cargo test --release --locked --test perf_gate "$phase" -- --nocapture

if command -v stackcollapse-perf.pl >/dev/null 2>&1 && command -v flamegraph.pl >/dev/null 2>&1; then
  echo "Rendering flamegraph..."
  perf script -i "$perf_data" | stackcollapse-perf.pl | flamegraph.pl > "$svg"
  echo "Artifacts:"
  echo "  $perf_data"
  echo "  $svg"
else
  echo "Note: stackcollapse-perf.pl and flamegraph.pl are not both on PATH;"
  echo "keeping the raw perf.data instead of an SVG flamegraph."
  echo "Artifacts:"
  echo "  $perf_data"
fi