# circuit-plugins Specification

## Purpose

Allow users to add DROID circuit definitions (and their rendering metadata) via TOML plugin files, merged over the embedded schema, so circuits shipped after the vendored schema snapshot can be validated, latency-modeled, and rendered without editing a submodule or rebuilding.

## Requirements

### Requirement: Plugin file discovery
The system SHALL load circuit definitions from `*.toml` files in `$XDG_CONFIG_HOME/droid-tui/plugins/` at startup, before rendering begins. A missing plugins directory is not an error.

#### Scenario: Default plugin directory
- **WHEN** the app starts and `$XDG_CONFIG_HOME/droid-tui/plugins/` does not exist
- **THEN** the app starts normally with only the embedded schema, and no warning is printed

#### Scenario: Plugin files present
- **WHEN** the app starts and `plugins/` contains `*.toml` files
- **THEN** every parseable file contributes its circuit definitions to the active schema before the first frame renders

### Requirement: Plugin circuit definition format
A plugin TOML file SHALL define circuits with: a circuit name, a category, a required `ramsize` in bytes, and input/output parameters (each with name, short, type, default, and the `prefix`/`count`/`start_at` expansion fields). Circuit names SHALL be case-insensitive like the embedded schema. A plugin file that omits a required field SHALL be skipped with a warning and must not prevent other files or circuits from loading.

#### Scenario: Valid plugin circuit
- **WHEN** a plugin file defines a circuit named `NEWCKT` with category, `ramsize`, and inputs/outputs
- **THEN** `NEWCKT` is available to validation, latency modeling, and rendering exactly like an embedded circuit

#### Scenario: Plugin missing ramsize
- **WHEN** a plugin file defines a circuit without a `ramsize`
- **THEN** the file is skipped, a warning naming the file and circuit is printed once, and startup continues

### Requirement: Merge with embedded schema
The system SHALL merge plugin circuits over the embedded schema. A plugin circuit whose name collides with an embedded circuit SHALL override it. When a plugin overrides an embedded circuit, the system SHALL warn once (per file) that the embedded definition is shadowed.

#### Scenario: Plugin overrides embedded circuit
- **WHEN** a plugin defines a circuit whose name matches an embedded circuit (e.g. `[copy]`)
- **THEN** the plugin definition wins for all consumers (validation, latency, rendering), and a single warning reports the shadowing

#### Scenario: Plugin with no collisions
- **WHEN** a plugin defines circuits whose names do not collide with the embedded schema
- **THEN** no shadow warning is emitted and all plugin circuits are additive

### Requirement: Declared cable kind and color
A plugin circuit definition SHALL optionally declare `cable_kind` (control, audio, midi) and a render `color` (a theme token name). When declared, the graph and panel renderers SHALL use them for edges and node coloring. When absent, the existing substring inference SHALL apply.

#### Scenario: Declared cable kind used
- **WHEN** a plugin circuit declares `cable_kind = "midi"` and produces a cable
- **THEN** that cable renders with the midi edge token

#### Scenario: Undeclared metadata falls back to inference
- **WHEN** a plugin circuit declares no `cable_kind` or `color`
- **THEN** the existing name-substring inference determines kind and color, with no error

### Requirement: ramsize drives RAM and latency
Plugin circuits SHALL participate in the `ram_overflow` validation check and the latency cost model using their declared `ramsize`, exactly like embedded circuits. Because `ramsize` is required, a plugin circuit must never silently disable RAM validation or fall back to the default latency cost.

#### Scenario: Plugin circuit counts toward RAM
- **WHEN** a patch uses a plugin circuit whose `ramsize` pushes the total over the schema's available memory
- **THEN** the `ram_overflow` check reports it as an Error

#### Scenario: Plugin circuit uses declared ramsize for latency
- **WHEN** a patch uses a plugin circuit and the latency view is active
- **THEN** the circuit's cost is derived from its declared `ramsize` (per the standard AVG formula), never the unknown-circuit default of 1.0
