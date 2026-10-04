# Tasks

## 1. Reliable arrangement core

- [x] 1.1 Fix pin-anchor carry-over to be NodeId-based across rebuilds and verify the reorder-anchor test passes <!-- agent: rusty-engineer.build, depends_on: [], touches: [src/app.rs] -->
- [x] 1.2 Make optimizer preview refit unconditional against the live graph-pane rect and verify the preview-refit test passes <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs] -->

## 2. Arrange key + docs

- [x] 2.1 Add graph-pane `a` key cycling Column-Strict → Column-Barycenter → Force with rebuild + refit + status and verify the cycle test passes <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs, src/handler.rs] -->
- [x] 2.2 Add the graph help-table row for `a` and verify the help test passes <!-- agent: rusty-engineer.fast, depends_on: [2.1], touches: [src/help.rs] -->

## 3. Verification

- [x] 3.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) and verify all four exit 0 <!-- agent: horst-engineer.fast, depends_on: [1.2, 2.2], touches: [] -->
