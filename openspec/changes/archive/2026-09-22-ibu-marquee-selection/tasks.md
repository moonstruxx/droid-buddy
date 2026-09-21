# Tasks: marquee selection into shared App selection

- [ ] 1. Commit marquee primary in `handle_graph_window_frame`
  - Files: `src/handler.rs` (plus unit tests in its test module)
  - When `frame.marquee` holds a non-empty node list, resolve the first
    enclosed scene index to its `NodeId` and call
    `App::select_circuit` unless it already equals
    `App::selected_circuit`. Map window-space rect indices through the
    same pane-origin and world mapping the click path uses so the primary
    matches the drawn highlight. Empty marquee: no-op. Do not touch the
    press, drag, release/pin, camera-key, or scene-rebuild branches.
  - Tests: marquee frame selects the primary node id and opens the tiled
    viewer slot; repeating the same marquee frame is idempotent (selection,
    scroll, cursor unchanged); empty marquee leaves selection alone;
    existing single-click test
    `graph_window_frame_press_selects_circuit` still passes unchanged.

- [ ] 2. Run the full verification gate set
  - `cargo fmt --check`, `cargo clippy --all-targets --all-features
    --locked -- -D warnings`, `cargo test --locked`, `cargo build
    --release --locked`. All four exit 0.
