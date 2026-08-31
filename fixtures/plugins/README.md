# Plugin format: DROID circuit plugins

Plugin files extend the embedded circuit schema (`ext/droid-lsp/droid-lsp/src/circuits.json`,
compiled into the binary via `include_str!` in `src/schema.rs`) with user-defined circuits.
A plugin is a TOML file, one `[[circuit]]` table per circuit definition. Files are loaded
at startup from `$XDG_CONFIG_HOME/droid-tui/plugins/*.toml`, in sorted filename order.

## Semantics

- Circuit names are case-insensitive, matching the embedded schema lookup. On a name
  collision the plugin definition replaces the embedded one and a warn-once shadow notice
  is printed. A file that fails to parse or validate (for example a circuit without
  `ramsize`) is skipped with a warning; startup never aborts because of a plugin.
- The embedded schema stays the base layer. No plugin files means behavior identical
  to today.
- Field names mirror the schema's serde shapes in `src/schema.rs`
  (`RawCircuitDef` / `RawParam`), so the plugin loader reuses and extends those structs.

## Circuit table

```toml
[[circuit]]
name = "NEWCKT"        # required, case-insensitive
category = "logic"     # required, free-form string
ramsize = 256          # required, integer bytes
title = "A new circuit"        # optional, defaults to the name
description = "..."            # optional, defaults to ""
cable_kind = "audio"           # optional: control | audio | midi
color = "knob"                 # optional: theme token name
```

Required fields: `name`, `category`, `ramsize`.

`ramsize` drives the `ram_overflow` validation check and the latency AVG cost model,
so a plugin circuit must declare it. Real DROID circuits use byte sizes in the
roughly 100-2000 range (the embedded `available_memory` for blue-7 is ~110k).

`cable_kind` (one of `control`, `audio`, `midi`) and `color` (a theme token name such
as `button`, `knob`, `cv_in`, `cv_out`, `led`) are declared rendering metadata. They are
consulted before the substring tables in `CableKind::from_circuit` / `circuit_color`;
when absent, those tables run unchanged (substring inference fallback). Embedded
circuits declare nothing and keep their current classification.

Fields from the embedded schema that plugins do not carry (`presets`, `manual`, and
the per-param `essential`, `ramhint`, `autotitle`) fall back to their neutral defaults
(`presets`/`manual` 0, `essential` 0, `ramhint` "", `autotitle` false).

## Parameters

Each circuit has `inputs` and `outputs` arrays. Each entry is a param table:

```toml
[[circuit.inputs]]
name = "input"         # required
short = "i"            # required
type = "jack"          # required, free-form string
default = "0"          # optional, string

[[circuit.outputs]]
name = "bit1 ... bit8" # expansion family: literal display name
short = "b"
type = "gate"
prefix = "bit"         # expansion trio, all three together
count = 8
start_at = 1
```

Required fields: `name`, `short`, `type`. `type` is a free-form string mirroring the
schema's `type` field (examples in the embedded schema: `cv`, `gate`, `trigger`,
`integer`, `bipolar`; a plugin may use others such as `jack`).

A numbered family of params is declared with the expansion trio `prefix` / `count` /
`start_at`, which must appear together. It expands to `count` params named
`<prefix><start_at>` .. `<prefix><start_at + count - 1>` (for example `prefix = "bit"`,
`count = 8`, `start_at = 1` yields `bit1` .. `bit8`); `name` then holds the display
family name (`bit1 ... bit8`), matching the embedded `adc` circuit's output layout.

## Files in this directory

- `valid.toml`: a new circuit (`NEWCKT`, jack input, `prefix`/`count` expansion, no
  declared metadata) and an override of the embedded `copy` circuit (declared
  `ramsize`, `cable_kind`, `color`).
- `missing_ramsize.toml`: a circuit without the required `ramsize`; the loader must
  skip the file and warn.
- `newckt_override.toml`: a second `newckt` definition (ramsize 512, category `util`)
  proving ordered overlay — loaded after `valid.toml` it wins for the same circuit
  name. Plugin-plugin collisions are a plain later-wins overlay, no shadow warning,
  because `newckt` is not an embedded circuit.