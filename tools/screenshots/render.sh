#!/usr/bin/env bash
# Deterministic README screenshot harness driver (change `readme-docs`, task 1.1).
#
# Renders docs/assets/*.png through the real egui paint path (headless wgpu),
# renders every shot twice, and fails when the two runs differ or when a PNG
# on disk does not match a fresh render. `--bless` rewrites the PNGs.
#
# Usage:
#   tools/screenshots/render.sh [--bless] [--only module-ui,graph] [--out DIR]
#
# Env:
#   CARGO_TARGET_DIR  reused when set, else a scratch dir under /tmp/opencode
#                     (keeps build output out of the repo; never tools/*/target).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/opencode/droid-screenshots-target}"
cd "$ROOT/tools/screenshots"
exec cargo run --offline --release -- --root "$ROOT" "$@"
