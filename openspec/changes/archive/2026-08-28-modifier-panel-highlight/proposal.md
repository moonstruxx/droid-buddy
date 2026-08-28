## Why

DROID patches encode hardware influence via `select = X` / virtual cables (`_VAR`). Today `modifier-trace` already walks the cable graph to highlight influenced circuits in the **graph** view and `select` spans in the **source** view, but the primary **panel** view (grouped P2B8 / Faderbank hardware) shows no signal awareness. Users cannot see which Buttons, Switches, Encoders and HW Ports a modifier actually drives. Reusing the structural forward-BFS to tint those cells — each modifier in its own hue while it is held — closes the hardware→signal loop without a new solver.

## What Changes

- Derives a per-HW-token influence set (`{ influenced HW tokens, influenced cables, influenced circuits }`) via the existing structural forward BFS (any `input+output` circuit is a hop, cycle-safe, deterministic, sorted) rooted at `_VAR`s produced by circuits that reference the token. **Structural only for this change** — `selectat` value gating and 8-way switch position awareness are deferred.
- Assigns a stable per-modifier hue by `hash(token) % 16` cycling the ANSI-16 palette (no new palette, no theme mutation beyond a pure helper). Collisions are tolerated; hue is advisory only.
- **Main panels (`controller-panels`):** influenced cells (boxed LED-cells and text cells inside modules) render with a background wash in the modifier hue. Orthogonal to `shift-visualization` which paints **panel borders**; both can coexist (shift border + modifier bg). Unaffected cells dim slightly when a modifier is active.
- **Interaction:** `Mouse Down` on a modifier-eligible component (Button/Switch/Encoder-press/HW Port/Knob that drives a `select`) = **momentary** highlight while held (cleared on `Up`); `Ctrl+Shift+Click` = **toggle latched** (additive union of multiple latched modifiers, each retaining its hue); `Esc` clears all latches and any momentary preview. Mirrors `active_shift` lifetime (`Esc` clears) but per-modifier.
- **Cross-view parity:** source `select` spans and graph edges/nodes for the same influence reuse the identical hue, plus a status hint `MOD B1.1 → 7 cells / 2 cables` in that hue.
- Influence cache is built once per patch load (`HashMap<token, Influence>`), pure and validated via unit/regression tests; no async, no IO.

## Capabilities

### New Capabilities

- `modifier-panel-highlight`: per-modifier hardware-cell highlighting in the main panel view driven by the structural signal trace, with momentary hold and additive Ctrl+Shift+Click latching, consistent hue across main/source/graph, and coexistence with shift borders.

### Modified Capabilities

- `controller-panels`: add requirement that influenced cells render with a modifier-hue background wash (and dim otherwise) while a modifier is active; geometry/hit-testing unchanged.
- `theming`: clarify modifier hue derivation as a pure `hash(token) % 16` over the active ANSI-16 palette; every theme keeps the hue advisory and distinct from the single `graph_edge_error` red; no new token required beyond a helper.
- `source-navigation`: extend modifier highlight from a single style to per-modifier hue (structural spans only; value gating remains out of scope).
- `signal-flow-graph`: extend edge/node influence highlight from a single style to per-modifier hue (additive union renders as multiple hued edges/nodes).

## Impact

- `src/patch.rs` — per-token influence cache (`HashMap<String, Influence>`) derived from `cable_index` + `circuit_outputs`; pure helper `modifier_color(token)`.
- `src/app.rs` — `pressed_modifiers` / `latched_modifiers` state, `influence_cache`, `Esc` clearing, status hint.
- `src/handler.rs` — mouse Down/Up + `Ctrl+Shift` chord detection, graph/source already own their mouse while open so panel chords only apply in main view.
- `src/ui.rs` — `render_component` / `render_component_grid` background wash, cluster/edge recolor in graph, span recolor in source, status bar hue.
- `src/theme.rs` — optional pure `modifier_hue(token) -> Color` helper; no new stored tokens, no config key.
- Tests: unit/regression for cache correctness, additive union, hue determinism, coexistence with shift, mouse chords; snapshots per theme/width for cell wash; gallery regeneration.

## Non-goals

- Value-aware filtering (`selectat = N`, switch position / pot value) — deferred to a follow-up after structural MVP proves the visual channel.
- Per-theme modifier palettes or config keys — `hash % 16` keeps YAGNI; theming stays three built-ins (`classic`/`terminal`/`mono`).
- Hardware bridge / MIDI SysEx upload, persistence of latch across patch loads, or network.
- Changes to panel geometry, hit-testing, or module grouping — only color is added.
