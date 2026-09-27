#!/bin/sh
# pc-repo-verify gate matrix for droid_tui (single crate, no source-roots.json)
# Runs the CI-defined checks in order, printing a marker + exit code per gate.
set -u
cd /home/bjoern/projects/droid_tui
check() {
  name="$1"
  shift
  echo "=== GATE START: $name ==="
  "$@"
  rc=$?
  echo "=== GATE EXIT: $rc $name ==="
}
git submodule status
check fmt      cargo fmt --check
check clippy   cargo clippy --all-targets --all-features --locked -- -D warnings
check test     cargo test --locked
check buildRel cargo build --release --locked
check perfGate cargo test --release --test perf_gate --locked
echo "=== ALL GATES DONE at $(date) ==="
