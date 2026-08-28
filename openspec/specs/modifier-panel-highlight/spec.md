# modifier-panel-highlight Specification

## Purpose
Structural per-modifier hardware highlighting that reuses the forward cable-graph walk to show, in the main hardware panels, which Buttons, Switches, Encoders, Pots and CV Ports a modifier drives — each modifier in its own stable hue with hold-vs-latch chords and additive unions.

## Requirements

### Requirement: Structural influence set per hardware token

The system SHALL compute, per patch load, a structural influence set for every hardware token that drives a `select` (directly `select = B1.1` or transitively via internal `_VAR`s), as the forward BFS over the cable graph: root `_VAR`s are those produced by circuits referencing the token (`output = _VAR` in same section), then `cable -> sink circuits (any param consuming _VAR) -> if sink has `output = _VAR` queue its outputs`. The walk SHALL be cycle-safe (visited cables/circuits), deterministic (sorted iteration), pure, and structural only (no `selectat` value gating, no switch-position gating; any `input+output` circuit on the flow is a hop).

#### Scenario: Direct consumption

- **WHEN** `B1.1` drives `_TRIG` and a circuit contains `select = _TRIG`
- **THEN** that circuit is in `B1.1`'s influence set and its consuming cable `_TRIG` is marked influenced.

#### Scenario: Indirect hop via switch

- **WHEN** `_SEL` is consumed by a `switch` (or any input+output circuit) that produces `_OUT`, and `_OUT` is consumed downstream
- **THEN** both the switch and the downstream circuit are in the influence set and both cables are marked influenced.

#### Scenario: Cycle safety

- **WHEN** cables form a cycle
- **THEN** the walk terminates, visiting each cable/circuit at most once.

#### Scenario: Token with no producer

- **WHEN** a hardware token never appears in a producer (`output = _VAR`) context
- **THEN** its influence set is empty and it is not highlighted as a modifier.

### Requirement: Stable per-modifier hue

The system SHALL assign each modifier token a stable hue derived as `hash(token) % 16` over the active ANSI-16 palette, deterministic per run and per patch (same token → same hue). The hue is advisory only and SHALL NOT introduce new theme tokens or config keys; `terminal` and `mono` themes map the hue to their available palette while keeping distinctness from `graph_edge_error` red.

#### Scenario: Determinism

- **WHEN** the same patch is loaded twice
- **THEN** `B1.1` maps to the same hue both times.

#### Scenario: Cycling

- **WHEN** more than 16 distinct modifiers exist
- **THEN** hues cycle (`% 16`) and collisions are tolerated.

### Requirement: Momentary hold highlight

While the user holds mouse `Down` on a modifier-eligible component (Button, Switch, Encoder-press, Pot, CV Port that is a known modifier driver) **without** `Ctrl+Shift`, the system SHALL highlight all cells/cables/circuits in that token's influence set in its hue (background wash in main panels, matching span/edge recolor in source/graph) and dim unaffected cells, clearing on `Up` or when the cursor leaves. No state persists after release.

#### Scenario: Hold and release

- **WHEN** the user presses `Down` on `B1.1` and holds
- **THEN** influenced cells show the `B1.1` hue wash until `Up`, then revert.

#### Scenario: Hold with no influence

- **WHEN** the held token has empty influence
- **THEN** no wash is applied and status shows `MOD B1.1 → 0 cells`.

### Requirement: Latched additive highlight via Ctrl+Shift+Click

The system SHALL toggle a latched highlight for the token on `Ctrl+Shift+Click` (Strg+Shift+Click). A latched token remains highlighted after release until `Esc` or a second `Ctrl+Shift+Click` on the same token removes it. Multiple latched tokens SHALL union (each cell/edge retains the hue of its influencing token; if a cell is influenced by two latched modifiers it shows the most-recently latched hue; additive status lists all latched tokens). `Esc` clears **all** latched modifiers and any momentary preview.

#### Scenario: Latch and persist

- **WHEN** the user `Ctrl+Shift+Click`s `B1.1`
- **THEN** its influence remains highlighted after release until `Esc`.

#### Scenario: Additive latching

- **WHEN** `B1.1` is latched and the user `Ctrl+Shift+Click`s `B1.2`
- **THEN** both influences are highlighted (union) with status `MOD B1.1+B1.2 → N cells`.

#### Scenario: Esc clears all

- **WHEN** one or more modifiers are latched or a hold is active and the user presses `Esc`
- **THEN** all modifier highlights are removed and status reverts (mirrors `SHIFT` clearing).

### Requirement: Cross-view hue parity and shift coexistence

The same modifier hue SHALL be used in main panels (cell background wash), source viewer (`select` line spans), and signal-flow graph (edges/nodes), plus a status bar hint `MOD <token> → N cells / M cables` in that hue. Modifier background wash SHALL be orthogonal to `shift-visualization` panel borders (shift paints panel border, modifier paints cell background) so both can be active simultaneously; rendering priority is modifier bg > dim, shift border remains visible.

#### Scenario: Shift plus modifier

- **WHEN** `Shift 1` (yellow panel border) is active and `B1.1` (cyan wash) is held
- **THEN** the panel shows yellow border with cyan cell backgrounds for influenced cells; unaffected cells are dimmed.

#### Scenario: Status hint

- **WHEN** a modifier is active (held or latched)
- **THEN** the status bar shows the token and counts in that token's hue.
