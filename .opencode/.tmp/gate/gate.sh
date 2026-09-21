#!/usr/bin/env bash
cd /home/bjoern/projects/droid_tui
LOG=.opencode/.tmp/gate
mkdir -p "$LOG"

echo "=== fmt ==="
cargo fmt --check > "$LOG/fmt.log" 2>&1
echo "fmt=$?"

echo "=== clippy ==="
cargo clippy --all-targets --all-features --locked -- -D warnings > "$LOG/clippy.log" 2>&1
echo "clippy=$?"

echo "=== test ==="
cargo test --locked > "$LOG/test.log" 2>&1
echo "test=$?"

echo "=== release ==="
cargo build --release --locked > "$LOG/release.log" 2>&1
echo "release=$?"

echo "=== summary ==="
grep -E "^test result:" "$LOG/test.log" | tail -20
tail -3 "$LOG/clippy.log"
tail -3 "$LOG/fmt.log"
tail -3 "$LOG/release.log"
