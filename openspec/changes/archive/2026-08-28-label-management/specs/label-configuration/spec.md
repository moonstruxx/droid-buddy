## Purpose

Exposes `[labels]` configuration for enabling per-shift HW label layers and capping the layer count, keeping overlay and display behavior deterministic without mutating patch files.

## ADDED Requirements

### Requirement: Labels config keys
The system SHALL expose `layers_enabled: bool` (default true) and `max_shift_layer: u8` (default 4, clamped 1..8) under `[labels]` in `config.toml` and persist them atomically.

#### Scenario: Clamped load
- **WHEN** `config.toml` has `max_shift_layer = 20`
- **THEN** effective `max_shift_layer` is 8 and the status/overlay only cycles 1..8

### Requirement: Disabled coercion preserves store
The system SHALL when `layers_enabled = false` coerce any `display_label(token, shift)` to layer 1 for reading while preserving 2..N entries in `labels.toml`.

#### Scenario: Disabled reads as singleton
- **WHEN** `layers_enabled = false` and store has `hw."B3.17".2 = "[RATC2]"`
- **THEN** rendering `B3.17` in Group 2 still shows Group 1 label `[RATC]` but editing later with `layers_enabled = true` can recover `[RATC2]`

