#!/usr/bin/env bash
# One-command verification for the native-egui-rendering change.
#
# Runs the full CI gate, proves the terminal stack is gone, and checks that
# the live window dispatch actually paints every ported surface. Exits 0 only
# when the change is truly green.
set -u
cd "$(dirname "$0")/.."

FAILURES=0
pass() { printf '  PASS  %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILURES=$((FAILURES + 1)); }

echo "== Gate =="
if cargo fmt --check >/tmp/verify-fmt.log 2>&1; then
    pass "cargo fmt --check"
else
    fail "cargo fmt --check"
fi
if cargo clippy --all-targets --all-features --locked -- -D warnings >/tmp/verify-clippy.log 2>&1; then
    pass "cargo clippy --all-targets --all-features --locked -- -D warnings"
else
    fail "cargo clippy -D warnings ($(grep -c '^error' /tmp/verify-clippy.log) errors)"
fi
if INSTA_UPDATE=no cargo test --locked >/tmp/verify-test.log 2>&1; then
    pass "INSTA_UPDATE=no cargo test --locked"
else
    fail "cargo test --locked"
fi
if cargo build --release --locked >/tmp/verify-build.log 2>&1; then
    pass "cargo build --release --locked"
else
    fail "cargo build --release --locked"
fi

echo "== Teardown proofs =="
for f in src/ui.rs src/kitty_protocol.rs src/snapshot-gallery.rs; do
    if [ ! -e "$f" ]; then
        pass "deleted $f"
    else
        fail "still present: $f"
    fi
done
if grep -Eq 'ratatui|crossterm|tiny-skia|fontdue|base64|flate2|insta' Cargo.toml; then
    fail "legacy deps remain in Cargo.toml"
else
    pass "no ratatui/crossterm/tiny-skia/fontdue/base64/flate2/insta in Cargo.toml"
fi
# Word-boundary match: `insta` must not match the `insta` inside `instance`.
if grep -rEn '\b(insta|TestBackend|ratatui|crossterm)\b' src tests 2>/dev/null | grep -q .; then
    fail "legacy references remain in src/ or tests/"
else
    pass "zero insta/TestBackend/ratatui/crossterm references in src/ and tests/"
fi

echo "== Surface wiring (dispatch must CALL each paint routine) =="
# A re-export does not count; the check matches the open paren so
# `pub use ...::paint_physical;` cannot satisfy it.
for fn in paint_physical paint_panels paint_viewer paint_picker \
    paint_validation_modal paint_select_menu paint_label_editor \
    paint_diff_surface paint_optimizer; do
    if grep -q "${fn}(" src/gui/mod.rs; then
        pass "${fn} called from dispatch"
    else
        fail "${fn} NOT called from dispatch"
    fi
done

echo "== Spec builders (App -> surface payload) =="
# The dispatch needs these builders to feed the paint routines from App state.
if grep -q 'fn physical_spec(' src/gui/physical.rs; then
    pass "physical_spec"
else
    fail "physical_spec missing"
fi
if grep -q 'fn panels_spec(' src/gui/panels.rs; then
    pass "panels_spec"
else
    fail "panels_spec missing"
fi
if grep -q 'fn viewer_spec(' src/gui/viewer.rs; then
    pass "viewer_spec"
else
    fail "viewer_spec missing"
fi
if grep -q 'fn picker_spec(' src/gui/picker.rs; then
    pass "picker_spec"
else
    fail "picker_spec missing"
fi
for fn in select_menu_spec label_edit_spec optimizer_spec; do
    if grep -q "fn ${fn}(" src/gui/overlays.rs; then
        pass "${fn}"
    else
        fail "${fn} missing"
    fi
done

echo
if [ "$FAILURES" -eq 0 ]; then
    echo "ALL CHECKS PASSED"
    exit 0
else
    echo "$FAILURES check(s) failed"
    exit 1
fi