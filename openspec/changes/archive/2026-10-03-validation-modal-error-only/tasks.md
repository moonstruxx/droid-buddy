# Tasks

## 1. Make warnings-only findings discoverable

- [x] 1.1 In `src/app.rs`, add the findings status hint to both load paths (`load_patch`, `load_patch_at`) for a Warning/Hint-only load — `Loaded <name> — N warnings/hints — press 'e' to view`, matching the Error path's existing `press 'e' to view` phrasing — while keeping `showing_validation = false`; and delete the dead `App::set_validation` (zero callers; re-verify with a repo-wide reference sweep before deleting). Cover both with in-module `#[cfg(test)]` tests asserting the hint text and the closed modal; run `cargo test --lib app` green <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->

## 2. Regression

- [x] 2.1 Lock the rule in `src/regression.rs`: a warnings-only fixture load (a `fixtures/validation/*.ini` that yields only Warning/Hint, otherwise the scale anchor `fixtures/droid_mpfs5melody2.ini`) leaves `showing_validation == false`, the status reports the count with the `'e'` hint, pressing `e` opens the modal, and `g g` still opens the graph; run `cargo test --lib regression` green <!-- agent: horst-engineer.build, depends_on: [1.1], touches: [src/regression.rs] -->

## 3. Verification

- [x] 3.1 Run the four gates and confirm each exits 0: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` <!-- agent: horst-engineer.fast, depends_on: [2.1], touches: [] -->
