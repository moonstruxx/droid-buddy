## 1. Startup fix

- [ ] 1.1 Remove unconditional `app.open_graph()` at startup in `src/main.rs` and verify `cargo test` passes
- [ ] 1.2 Update `startup_seeding_produces_a_nonempty_scene` test to assert startup without graph (or explicitly call `open_graph` after load for the non-empty assertion) and verify `cargo test` passes
- [ ] 1.3 Live visual verification: start app, confirm initial view is physical/panels (no graph), press `g g` to open graph, drag a node and confirm window stays responsive (no OS "not responding" dialog)

## 2. Verification

- [ ] 2.1 Run full verification gate `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` and verify all pass
