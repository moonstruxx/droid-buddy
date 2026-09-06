## 1. Seed wiring

- [x] 1.1 `seed_app` seeds `graph_window_enabled` from `settings.gui.graph_window` under `#[cfg(feature = "gui")]` in src/main.rs, matching the existing `physical_rack_spec` seeding contract <!-- agent: rusty-engineer.fast, depends_on: [], touches: [src/main.rs] -->
- [x] 1.2 Add a `gui`-gated unit test asserting `seed_app` sets `graph_window_enabled` from a `[gui] graph_window = true` setting (and keeps the default `false`) <!-- agent: horst-engineer.fast, depends_on: [1.1], touches: [src/main.rs] -->

## 2. Verification gate

- [x] 2.1 Run `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test --locked`, `cargo test --all-features --locked`, and `cargo build --release --locked`; all must exit 0 <!-- agent: rusty-engineer.fast, depends_on: [1.2], touches: [] -->