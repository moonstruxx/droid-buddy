# Tasks

## 1. Upload core

- [x] 1.1 SysEx payload builder (pure `src/sysex.rs`: memory patch → stripped 7-bit text → `F0 00 66 66 50 … F7` framing per the `droidpatch` rules) and verify the framing tests pass <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [src/sysex.rs] -->
- [x] 1.2 Confirm modal + `U` key + transport detect + background send with waiting bar and verdict (preflight probe, `y`/`n`/`Esc`, child reaped on quit) and verify the flow tests pass <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/app.rs, src/handler.rs, src/gui/overlays.rs] -->

## 2. Docs

- [x] 2.1 Add the help-table rows (`U`, modal keys) and verify the help test passes <!-- agent: rusty-engineer.fast, depends_on: [1.2], touches: [src/help.rs] -->

## 3. Verification

- [ ] 3.1 Run the full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked`) and verify all four exit 0 <!-- agent: horst-engineer.fast, depends_on: [1.1, 1.2, 2.1], touches: [] -->
