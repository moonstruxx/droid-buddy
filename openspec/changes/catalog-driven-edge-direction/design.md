## Context

The graph model (graph.rs) builds a directed edge per cable from `Patch.cable_index`, which `collect_cable_index` (patch.rs) fills. That function decides a parameter is a producer by checking whether its key is literally `output`. The embedded schema (schema.rs) already distinguishes inputs from outputs: `CircuitDef.inputs` and `CircuitDef.outputs` are split on each parameter's `type`, and `Schema::get_param_kind` resolves a circuit and parameter key to `"output"`, `"input"`, or `None` after prefix/count expansion. That helper exists but has no consumer.

## Goals / Non-Goals

**Goals:**

- Make producer vs sink come from the catalog, with the `output` convention as a fallback for unknown circuits and parameters.
- Keep the 76 embedded circuits and all existing fixtures byte-identical.

**Non-Goals:**

- Drawing register, jack, or controller nodes.
- Touching the influence traversal beyond reusing the same classification.
- Removing the unused `get_output_param_names` helper.

## Decisions

- **Use `Schema::get_param_kind` for the classification.** It returns a three-way result (output, input, none) that maps directly to producing, consuming, or unknown. The alternative `get_output_param_names` only answers "is output", so it cannot express that a catalog input parameter must never produce a source and would leave the input branch implicit.

- **Call `load_schema()` inside the two patch.rs functions.** `collect_cable_index` and `collect_circuit_outputs` are free functions with no schema argument. Threading a `&Schema` through the public `Patch::from_ini_*` entry points would widen the parser API for no gain: the schema is a process-wide cache and validation already pulls it on every load.

- **Fall back to `key == "output"` when `get_param_kind` returns `None`.** A `None` means the circuit or parameter is absent from the catalog, which is exactly where the old convention must survive (synthetic test sections, plugin gaps). This keeps the change non-breaking and makes the catalog purely additive for known circuits.

- **Change `collect_circuit_outputs` too.** It builds the same "what does this section produce" fact for the influence reverse map. Leaving it on `output` while the cable index goes catalog-driven would let the graph and the influence view disagree about which cables a circuit produces. Both should read the same source.

## Risks / Trade-offs

- [A known circuit uses a key the catalog does not list as an output, and a fixture asserts `output = _X` is a source] → the fallback keeps the `output` convention for unknown parameters too, so no fixture regresses. A regression test runs every existing fixture unchanged.

- [A catalog change to a circuit's output parameters silently changes edge direction] → direction is meant to be catalog-authoritative; that is the point of the change, not a defect.

- [Parsing now depends on the schema cache] → `load_schema` is lazy and cached, validation already calls it per load, and the test-override hook lets schema-dependent tests pin a fixture.

- [A nonstandard output parameter is a rare corner] → the embedded circuits conventionally name their primary output `output`, but several declare extra output params with other names (`button longpress/inverted/negated/shortpress`, `algoquencer trigger/pitch/accent`, `motoquencer cv/gate/startofsequence`, `logic or/and`, `pot lefthalfinv/righthalf/absbipolar/bipolar`, `slew linear/exponential`, `sequencer gateoutput`, `gatetool outputgate`, `encoder button`, `clocktool inputpitch`, `bernoulli output1/output2`, `spring position/velocity`, `lfo square`). Existing fixtures use these as bare `_NAME` sources, so the catalog-driven source classification has real embedded coverage; the schema-level fixture in the unit test keeps the exact input/output/unknown tri-state assertion deterministic.
