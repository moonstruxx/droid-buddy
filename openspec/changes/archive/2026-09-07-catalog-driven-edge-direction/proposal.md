## Why

The graph builds every cable edge by guessing that a circuit's producer parameter is named `output`. The embedded DROID schema already records, per circuit, which parameters are outputs and which are inputs. Reading that catalog removes the guess, and it is the same authority a later change needs in order to draw register, jack, and controller nodes.

## What Changes

- Cable direction now comes from the circuit's catalog definition instead of the literal `output` key. A parameter whose key resolves to a catalog output parameter creates a cable source when its value is a bare cable name, and a sink when its value is an expression that reads another cable.
- A parameter the catalog marks as an input never creates a source.
- For a circuit or parameter the catalog does not know, the `output = _NAME` convention is kept unchanged. This preserves the behavior of the 76 embedded circuits and every existing fixture.
- The per-section output reverse map (`collect_circuit_outputs`) uses the same catalog lookup, so the influence traversal agrees with edge direction.

## Capabilities

### New Capabilities

- (none)

### Modified Capabilities

- signal-flow-graph: the "Virtual-cable extraction" requirement changes to derive producer vs sink from the catalog, with the `output` fallback preserved.

## Impact

- src/patch.rs: `collect_cable_index` and `collect_circuit_outputs` call `load_schema().get_param_kind` instead of comparing the key to `output`.
- src/schema.rs: `get_param_kind` gains its first consumer (it is currently unused). `get_output_param_names` remains unused and is redundant with `get_param_kind`.
- No new dependencies. Parsing now touches the schema cache, which is cheap after the first load and already pulled in by validation on every patch load.

## Non-goals

- No register, jack, or controller nodes in the graph; that is a later change.
- No change to layout solving, edge rendering, or diff and influence coloring.
- No removal of the pre-existing `get_output_param_names` helper.
