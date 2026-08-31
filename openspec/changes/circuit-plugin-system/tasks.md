# Circuit Plugin System — Tasks

## 1. Plugin format & loader

- [x] 1.1 Define plugin TOML format from DROID circuit semantics (name/category/ramsize required; inputs/outputs with prefix/count/start_at; optional cable_kind + color) and add `fixtures/plugins/` with a valid circuit file and a malformed (missing ramsize) file
  <!-- agent: dermannmitdermachine-engineer.build, depends_on: [], touches: [fixtures/plugins/**] -->
- [x] 1.2 Implement `src/plugin.rs` — XDG discovery (`$XDG_CONFIG_HOME/droid-tui/plugins/*.toml`), parse via serde, per-file validation (ramsize required, case-insensitive names), sorted-filename load order; skip malformed files with warn-once, never abort startup; wire module into `src/lib.rs`
  <!-- agent: rusty-engineer.build, depends_on: [1.1], touches: [src/plugin.rs, src/lib.rs] -->
- [x] 1.3 Implement merge into `Schema` — embedded base + plugin overlay (insert-or-override on collision, warn-once on shadow per file); verify a colliding plugin circuit overrides an embedded one and a non-colliding one is additive
  <!-- agent: rusty-engineer.build, depends_on: [1.2], touches: [src/plugin.rs, src/schema.rs] -->
- [x] 1.4 Add loader + merge unit tests: discovery with/without dir, valid file loads, missing ramsize skips + warns, collision overrides + shadow warn, case-insensitive names; verify `cargo test plugin` passes
  <!-- agent: horst-engineer.build, depends_on: [1.3], touches: [src/plugin.rs, fixtures/plugins/**] -->

## 2. Cached merged schema

- [x] 2.1 Rework `load_schema()` to return a cached `&'static Schema` behind `Mutex<Option<&'static Schema>>` (theme's pattern — Mutex not OnceLock), with on-demand fallback parse of the embedded JSON when uninitialized; verify the 6 production call sites compile unchanged (deref coercion) and existing schema tests pass
  <!-- agent: rusty-engineer.build, depends_on: [1.3], touches: [src/schema.rs] -->
- [x] 2.2 Add `schema::init()` (parse embedded + merge plugins into the global, honoring `[plugins].enabled`/`dir`) and call it from `main()` before `ratatui::init()` so plugin warnings land on a clean terminal (ADR 14)
  <!-- agent: rusty-engineer.fast, depends_on: [2.1], touches: [src/main.rs, src/schema.rs] -->
- [x] 2.3 Add cache/init tests — idempotent init, uninitialized fallback parse, no cross-test poisoning (folded into 2.2: uninitialized-fallback + no-poisoning already landed in 2.1) — verify `cargo test schema` passes
  <!-- agent: horst-engineer.fast, depends_on: [2.1], touches: [src/schema.rs] -->

## 3. Declared metadata replaces name inference

- [x] 3.1 Extend the circuit definition with optional `cable_kind` + `color` fields; update `CableKind::from_circuit` to consult the declared kind first, substring tables as fallback; verify embedded circuits classify byte-for-byte (existing kind tests unchanged and green)
  <!-- agent: layout-designer-engineer.build, depends_on: [1.3, 2.1], touches: [src/ui.rs, src/schema.rs] -->
- [x] 3.2 Update `circuit_color` the same way (declared color token first, substring inference fallback); verify existing color tests unchanged and green (folded into 3.1 commit `cbe59f5`: `kind_token_color` helper + `circuit_color_prefers_declared_token_over_name` test)
  <!-- agent: layout-designer-engineer.build, depends_on: [3.1], touches: [src/ui.rs] -->
- [x] 3.3 Add visual proof: plugin-circuit fixture (patch whose producing circuit declares cable_kind + color) into the gallery/snapshot matrix across themes/widths; verify `cargo test` snapshot gate passes with the new row rendering the declared kind/color
  <!-- agent: horst-engineer.build, depends_on: [3.2], touches: [src/regression.rs, fixtures/plugins/**, src/bin/snapshot-gallery.rs] -->

## 4. Validation integration

- [x] 4.1 Verify plugin circuits participate in all 9 checks incl. `ram_overflow` (patch using a plugin circuit must no longer skip RAM checks); add regression test that a plugin circuit over budget reports `ram_overflow` Error and one within budget validates clean; verify `cargo test validation` passes
  <!-- agent: horst-engineer.build, depends_on: [2.2], touches: [src/validation.rs, fixtures/plugins/**] -->
- [x] 4.2 Add optional `[plugins]` config section (`dir` override, `enabled` bool default true, malformed → warn-once + defaults) to `config.rs`; verify config tests cover absent/disabled/custom-dir
  <!-- agent: rusty-engineer.fast, depends_on: [1.2], touches: [src/config.rs] -->

## 5. Verification & docs

- [ ] 5.1 Update ARCHITECTURE.md (plugin.rs module, merged-schema cache, declared metadata) and DESIGN.md (plugin format + merge semantics); sync the four delta specs and new `circuit-plugins` spec
  <!-- agent: devops-engineer.fast, depends_on: [3.3, 4.1, 4.2], touches: [ARCHITECTURE.md, DESIGN.md, openspec/specs/**] -->
- [ ] 5.2 Full verification gate: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` — all four exit 0
  <!-- agent: horst-engineer.fast, depends_on: [5.1], touches: [] -->