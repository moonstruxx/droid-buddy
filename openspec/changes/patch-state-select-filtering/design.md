## Context

A circuit binds to hardware through `select = <signal>` and `selectat = <value>`: when the signal equals the `selectat` value, the circuit owns the buttons, LEDs, and faders it names. `Patch` already tracks select/selectat pairs in the modifier index (`build_modifier_index`, cycle-safe), and the parser records every section's entries with their raw values. Change B draws all register edges unconditionally; this change gates the controller edges on an assumed select state.

DroidGraph's model, which this ports:
- `StateControls.Discover` lists every signal referenced by a `select` input, ranked by usage, with candidates: literal `selectat` values (when the circuit's select is that signal alone), the `valueN` outputs of button groups, and a `0`/`1` default pair.
- `StateControls.Evaluate` classifies a circuit as Selected (no select input, or the expression evaluates to the assumed value), NotSelected (evaluates to something else), or Unknown (unevaluable).
- The graph build drops the controller connections of NotSelected circuits but keeps their CV, gate, and cable edges, because an unselected circuit keeps running.

Its `ExpressionEvaluator` is a strict left-to-right evaluator: Droid expressions have no operator precedence and no parentheses; `1V` means 0.1 (10 V is the internal value 1.0); any token that is neither a literal, a cable, nor a register with an assumed value aborts the evaluation (null = unknown).

## Goals / Non-Goals

**Goals:**

- Port the evaluator and the discovery pass, evaluate per-circuit selection against an assumed state, and rebuild the graph with NotSelected circuits dropping their controller register edges.
- Offer the candidate values in a menu so the user can answer "which circuit owns this hardware right now" without editing the patch.

**Non-Goals:**

- Editing or persisting the assumed state; it is per-session and resets on patch load like the modifier influence.
- Full DROID expression semantics; plain left-to-right arithmetic is the scope, matching what real select expressions use.
- Unifying select state with shift groups or the modifier influence model.

## Decisions

- **The evaluator lives in a new pure module.** `src/expression.rs` holds `evaluate_droid_expr(expr, values) -> Option<f64>` with a small tokenizer: literals (with optional `V` suffix dividing by 10), `_CABLE` names, register tokens (via change B's `scan_register_refs`), and `+ - * /` operators. It is anchored (any unrecognized token aborts), mirrors DroidGraph's zero-divisor rule (division by zero yields 0), and treats a leading minus as `0 - operand`. Being a separate module keeps it unit-testable without pulling in patch or schema state.

- **Discovery reuses the parser's select/selectat data, not a new scan.** Every section with a `select` entry contributes its referenced signals (registers via `scan_register_refs`, cables via `scan_internal_tokens`). When the whole select value is a single signal, the section's `selectat` literal becomes a candidate for that signal. Candidate sets are completed with the `0`/`1` default pair and with the literal values of `valueN` outputs of button-group circuits driving the signal. Signals are ranked by descending usage, then name, for a stable menu order.

- **Selection is evaluated per circuit at graph build.** `GraphOptions { state: HashMap<String, f64>, hide_unselected: bool }` flows into `build_from_patch`. A circuit with no `select` entry is Selected. Otherwise the select expression is evaluated against the state: equal to the assumed value of its root signal (or to any assumed value when the expression is a bare signal) is Selected, unequal is NotSelected, unevaluable is Unknown. NotSelected circuits skip their controller register edges (the `on_controller` branch of change B) but keep jack and cable edges; with `hide_unselected` they are dropped from the node set entirely.

- **The assumed state is a menu, not a keyboard-per-value prompt.** `g s` opens a centered list of discovered signals (token, kind, candidates, usage count, current value). `j`/`k` navigate, `[`/`]` cycle the candidate values of the focused signal (wrapping), and the graph rebuilds on every change. Esc closes and clears the state, restoring the unassumed graph. Unset signals are absent from the map, so expressions referencing them evaluate Unknown rather than a guessed value. The status bar shows the selection summary (`Select state: N selected / M unselected / K unknown`).

- **NotSelected rendering reuses `graph_node_dim`.** The node dims and its controller edges vanish; cable and jack edges keep their normal colors so the signal flow remains readable. The dim token already exists and is used for disabled circuits; sharing it keeps the visual language consistent. Unknown circuits render normally, since no claim is made about them.

## Risks / Trade-offs

- [A real select expression uses syntax the evaluator does not cover] → it evaluates Unknown, the circuit keeps its edges, and the menu simply does not claim a state for it. Conservative by construction.

- [The `selectat` candidate set misses a value the hardware actually uses] → the `0`/`1` defaults and button-group `valueN` inference cover the common cases; the menu shows every discovered candidate and the user cycles to the right one.

- [Dropping controller edges hides wiring the user wants to see] → the `hide_unselected` flag is off by default; the unassumed graph (Esc) restores every edge. The dim rendering keeps the dropped connections visually present as dimmed nodes.

- [Selection evaluation couples graph build to expression evaluation] → both are pure functions over the parsed patch; the coupling is one `Option<HashMap>` parameter with a default of no state, and all existing callers are unaffected.