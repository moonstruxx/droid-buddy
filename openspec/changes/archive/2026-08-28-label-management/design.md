## Context

Single-crate ratatui TUI (`patch.rs` pure domain, `config.rs` XDG, `app.rs` state, `handler.rs` input, `ui.rs` render). Quad view already exists (panels | source, graph FULL | FILTERED) with `influence_subtree` BFS and `modifier_hue`. Label editing is an overlay over that quad; no `.ini` mutation, no async/network.

## Goals / Non-Goals

Goals: per-patch XDG `labels.toml` for HW per-shift + circuit per-instance, `[labels]` config (`layers_enabled`, `max_shift_layer=4`), single-field overlay with `1..N` cycle, title/header overrides, structural highlight, atomic warn-once.
Non-Goals: `.ini` writes, N>8 layers, Acified N-bag menu paging, shift-aware `selectat`, MIDI/hardware, ML.

## Decisions

- Store: `~/.config/droid-tui/labels.toml` TOML with ` [patches."/abs/path"] { hw."B3.17" = {1="...",2="..."}, circuits."motorfader:12"= "..." }` keyed by canonicalized absolute path String. Atomic tmp→rename, warn-once on corrupt (log + fallback empty), mirror of `config.rs`. Hash follow-up noted but not shipped.
- Config: `Settings.labels { layers_enabled: bool, max_shift_layer: u8 }` in `config.rs`, defaults `true`/`4`, clamp 1..8 on load/save. Disabled ⇒ `display_label` coerces shift→1 (read-only), store 2..N preserved. Saves via same atomic pattern.
- Domain: `Patch::display_label(token, shift)` pure, shift clamped 1..=max then optionally coerced; chain `store[layer]→store[1]→preamble[1]→derived`. `Patch::circuit_label(NodeId)` for graph/source. No I/O in `patch.rs`; `LabelStore` in `app.rs` owns path→toml I/O.
- App: `App { label_store: LabelStore, editing: Option<EditState> }` where `EditState { kind: Hw(token,layer) | Circuit(NodeId), draft: String }`. `load_patch` loads bucket; `Enter` mutates `label_store` + atomic save; `Esc` cancels; `recompute_influence` still drives status hue.
- Input: `handler.rs` priority `overlay_eating → picker → prefix → graph → source → panels`. `e` enters overlay for focused datum; while overlay open, chars append to `draft`, `1..N` switches Hw layer (preserving per-layer draft in a small `BTreeMap`), `Enter` commits, `Esc` exits, arrows/Home/End optionally.
- Render: `ui.rs` draws quad then overlay z-layer (centered 1-line input + hint `Group2 → ...` in modifier hue). Overrides applied before boxed/TextCell render (panels), header line (source), node frame title (graph both panes). No geometry change; `component_rects`/`graph_node_rects` unchanged.

## Risks / Trade-offs

- Path-key fragility on move/rename → noted migration to content hash; mitigated by documented key + preserved store entries.
- Overlay steals alphanumeric keys while open → acceptable modal; `Esc` always returns.

## Migration Plan

Additive TOML; missing `[labels]` → defaults. Empty `I4:` still falls through derived. No data migration.

## Open Questions

- None blocking; `graph_edge_error` precedence already defined.
