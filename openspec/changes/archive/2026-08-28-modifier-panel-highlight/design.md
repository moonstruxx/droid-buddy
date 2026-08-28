## Context

`droid_tui` is a single-crate layered monolith (main → app → handler → patch → ui) with pure domain parsing (`patch.rs`), a deterministic force-directed layout (`layout.rs`), and a synchronous observer bus (`events.rs`). Existing modifier awareness lives in two places: `patch::modifier_index` (transitive `select = X` resolution for source highlights, cycle-safe) and `modifier-trace`'s forward BFS over `cable_index` (structural `cable→sink→output` walk for graph highlights). Today both highlight only source spans or graph edges/nodes, both in a single hue. Shift groups (`1-4`) paint **panel borders**; panels contain modules → components with fixed `16×3` cell geometry and per-frame `component_rects` hit-testing. See proposal.md Why and specs for full requirements.

## Goals / Non-Goals

**Goals:**
- Reuse the structural BFS to produce a per-token `Influence { hw_tokens, cables, circuits }` cache at patch load, consumed identically by main/source/graph with consistent `hash % 16` hue.
- Additive momentary (hold) vs latched (Ctrl+Shift+Click) interaction without new threads, timers, or geometry changes.
- Orthogonal rendering: modifier = cell **background** wash, shift = panel **border** — both visible.

**Non-Goals:**
- Value-aware filtering (`selectat`, pot/switch position) — structural walk only for MVP.
- New theme tokens, config keys, or palette storage — hue is a pure function.
- Persistence of latch across patch loads, hardware bridge, or layout/hit-test changes.

## Decisions

### Decision: Influence cache in `patch.rs` + `app.rs` reference, not recomputed per frame

- **Choice:** At `Patch::from_ini_str` build a `HashMap<Token, Influence>` where each `Influence` holds `HashSet<String>` hw_tokens (those appearing as param values in influenced sink circuits), `HashSet<String>` cables, `HashSet<NodeId>` circuits. Walk uses `cable_index` + `circuit_outputs` (parallel to `sections`) exactly as `graph::build_from_patch` does, but seeds from `derive_root_vars(token)` (circuits where token appears and produces `output = _VAR`). Cached once; `App` holds `Option<InfluenceCache>` and clears on next `load_patch`.
- **Alternative:** compute on demand per mouse event — rejected (repeated BFS, jank on held drag).
- **Rationale:** pure, testable without terminal, O(#modifiers * walk) at load (dozens of tokens, cheap).

### Decision: `hash(token) % 16` hue, not stored palette

- **Choice:** `theme::modifier_hue(token: &str) -> Color` as `let idx = fxhash(token) % 16` mapping to `[Yellow, Cyan, Magenta, Green, Blue, Red, LightYellow, LightCyan, LightMagenta, LightGreen, LightBlue, LightRed, White, Gray, DarkGray, Reset]` (classic mapping; `terminal`/`mono` remap via existing token sets). No `Theme` field added — YAGNI.
- **Alternative:** add `modifier_palette: [Color; 4]` to `Theme` — rejected (needs per-theme design, config surface).
- **Rationale:** deterministic, zero storage, cycles naturally; `terminal` stays Reset-tolerant.

### Decision: App state — held vs latched sets

- **Choice:** `App { influence_cache, pressed: Option<String> (current hold), latched: BTreeSet<String> (ordered for determinism), latched_order: Vec<String> (insertion order for most-recent wins) }`. `pressed` set on `MouseDown` on `component_rects` hit that is modifier-eligible (token has non-empty influence); cleared on `MouseUp`/`MouseLeave`. `Ctrl+Shift+Click` toggles membership in `latched`. `Esc` clears both and any shift. Status hint derived from union of `pressed ∪ latched`.
- **Alternative:** single `active: HashSet` — rejected (cannot distinguish momentary vs persist).
- **Rationale:** mirrors `active_shift: Option<ShiftGroup>` but per-token and additive; no timer thread (reuses existing lazy prefix pattern).

### Decision: Rendering — cell background wash

- **Choice:** `ui::render_component` / `render_component_grid` takes `modifier_hue: Option<Color>`; when `Some`, `Style::bg(hue).add_modifier(BOLD)` on the cell interior (boxed cells wash the single interior row, text cells wash both lines). Unaffected cells while any modifier active get `Modifier::DIM`. Source spans and graph edges/nodes use the same hue via `theme::modifier_hue` (source: `Span` style, graph: `cable_color` override where modifier outranks `CableKind` but yields to `graph_edge_error` red).
- **Alternative:** recolor border — rejected (collides with shift border intent).
- **Rationale:** orthogonal channel keeps shift visible; per-cell cost is O(n) similar to existing render.

### Decision: Handler chord — Ctrl+Shift+Click

- **Choice:** `crossterm::event::KeyModifiers::CONTROL | SHIFT` on mouse event for latch toggle; `Shift` alone is not enough (avoids confusion with DROID `1-4` shift), `Ctrl` alone is free today (only `Ctrl+C` quit). `Down` without modifiers = momentary.
- **Alternative:** `Shift+Click` — rejected due to naming collision DROID shift vs keyboard Shift.
- **Rationale:** explicit chord, discoverable via viewer status hint `Ctrl+Shift+Click to latch`.

## Risks / Trade-offs

- **Hue collisions (16 cycle) → low distinguishability when many modifiers latched** → Mitigation: most-recent wins plus status list of latched tokens in text; additive wash limited to 3–4 visible hues in practice (patches rarely drive >5 distinct selects).
- **Structural over-approximation (e.g., switch with 8 inputs all marked influenced)** → Mitigation: documented as MVP limitation; follow-up will gate by `selectat`/current value, no API break since hue channel stays.
- **Mouse modifier reporting varies by terminal (Ctrl+Shift may not arrive)** → Mitigation: also accept `Ctrl+Click` as alias in handler; keyboard fallback `m` toggles influence for hovered component in main view.
- **Graph edge hue vs CableKind vs error precedence** → Mitigation: priority `error red > modifier hue > CableKind`; single match arm in `ui::cable_color`.
- **Cache invalidation on patch reload** → Mitigation: rebuild on `load_patch`; no incremental update needed.

## Migration Plan

No migration. Additive visual channel, no stored state, no config. `cargo test` + `cargo insta test --check` gate; snapshot updates accepted via `INSTA_UPDATE=always`. Gallery regeneration covers per-theme/width washes. Rollback is revert of the 4 touched modules.

## Open Questions

- None blocking — `selectat`/switch value gating is captured as a named follow-up issue (structural MVP proves the rendering pipeline first).
