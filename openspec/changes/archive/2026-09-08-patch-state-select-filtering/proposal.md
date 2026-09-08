## Why

A DROID patch often shares one controller across several circuits that select on a switch or button (`select = S7.1`, `selectat = 0.5`). The circuit whose select matches "owns" the buttons, LEDs, and faders it names; the others keep running but do not touch that hardware. The graph today draws every register edge (change B) regardless of which circuit currently owns the hardware, so it shows connections that are false for any concrete state of the patch. DroidGraph solves this by discovering the select signals, offering candidate values, and rebuilding the graph for an assumed state: circuits whose select does not match drop their controller connections while their cable and CV/gate edges stay intact. This change ports that behavior. It answers the question the current graph cannot: which circuit owns my B32 right now.

## What Changes

- A left-to-right Droid expression evaluator (`1V` means 0.1, no operator precedence, no parentheses) evaluates `select`/`selectat` expressions against assumed values for the signals they reference.
- A discovery pass lists every signal referenced by a `select` input anywhere in the patch, ranked by how many circuits select on it, with the values it plausibly takes: the literal `selectat` values of circuits whose select is that signal alone, the `valueN` outputs of button groups, and the default `0`/`1`.
- An assumed state (`HashMap<String, f64>`) selects a value per discovered signal. Each circuit is then Selected, NotSelected, or Unknown: Selected when it has no select input or its expression evaluates to the selected value, NotSelected when it evaluates to something else, Unknown when the expression cannot be evaluated.
- The graph build takes the assumed state: NotSelected circuits lose their controller register edges (change B) but keep their cable, CV/gate, and jack edges; they render dim. An optional mode hides them entirely. Unknown circuits keep their edges and render normally.
- A `g s` menu lists the discovered signals; `j`/`k` navigate, `[`/`]` or Enter cycles the candidate values, the graph rebuilds live, Esc closes and restores the unassumed graph.

## Capabilities

### New Capabilities

- signal-flow-graph: a new requirement "Select-state filtering" covering expression evaluation, signal discovery, the assumed-state menu, and per-circuit selection rendering.

### Modified Capabilities

- signal-flow-graph: the "Register and hardware nodes" requirement (change B) gains the rule that a NotSelected circuit drops its controller register edges.

## Impact

- New module `src/expression.rs` (or a section of patch.rs): the Droid expression evaluator, pure and unit-tested.
- src/patch.rs or src/graph.rs: select-signal discovery and per-circuit selection evaluation over the parsed sections.
- src/graph.rs: `GraphOptions { state, hide_unselected }` feeding the register-edge pass from change B.
- src/app.rs: `select_state: Option<SelectState>` holding the discovered signals and the assumed values; graph rebuild on state change.
- src/handler.rs: the `g s` menu keys.
- src/ui.rs: the menu rendering and dim rendering for NotSelected nodes (reusing `graph_node_dim`).
- src/theme.rs: no new tokens required (reuses `graph_node_dim`); the menu reuses the validation-modal border tokens.

## Non-goals

- No unification with the shift-group or modifier-influence model; select state is a separate axis.
- No editing of the patch or persisting the assumed state.
- No evaluation of select expressions with full DROID semantics (sample-and-hold, comparisons); the evaluator covers plain arithmetic, which is what real patches use.
- No change to the influence traversal or the diff model.