# label-management Specification

## Purpose
Provides per-patch user-editable HW per-shift and per-circuit labels stored in XDG labels.toml, with inline quad editing and structural influence-aware status without mutating .ini files.

## Requirements

### Requirement: HW per-shift label store and fallback
The system SHALL store HW token labels per patch and per shift slot `1..=max_shift_layer` in `labels.toml` and resolve `display_label(token, shift)` as `store[layer] → store[1] → preamble[1] → derived "Button B3.17"`.

#### Scenario: Store overrides preamble
- **WHEN** `labels.toml` has `hw."B3.17".2 = "[RATC2]"` and preamble has `B3.17: [RATC]`
- **THEN** `display_label("B3.17",2)` is `[RATC2]` and `display_label("B3.17",1)` is `[RATC]`

#### Scenario: Empty I4 fallback
- **WHEN** preamble has `I4:` empty and no store entry for `I4` and derived is `Input I4`
- **THEN** `display_label("I4",2)` falls back to derived `Input I4` via `store[2]→store[1]→preamble[1]`

### Requirement: Circuit per-instance label store
The system SHALL store one label per `NodeId=(circuit,instance)` in `labels.toml` under `circuits."<circuit>:<instance>"` and use it as source/graph title override.

#### Scenario: Circuit override
- **WHEN** `circuits."motorfader:12" = "T1 Accu"` is set
- **THEN** source header for `motorfader:12` and graph node title for that instance show `T1 Accu` instead of `motorfader`

### Requirement: Per-patch isolation and atomic persistence
The system SHALL key store buckets by canonicalized absolute patch path and SHALL persist via atomic tmp→rename with warn-once on corrupt toml (fallback empty).

#### Scenario: Two masters isolated
- **WHEN** `droid_mpfs5melody2.ini` and `droid_mpfs5drum.ini` both have `B1.1`
- **THEN** editing `B1.1` label in one patch does not affect the other file's labels

### Requirement: Inline edit overlay with layer cycle
The system SHALL open a single-field overlay on `e` for the focused datum (panels token / source header instance / hovered graph node) and allow `1..max_shift_layer` to cycle the edited HW layer inside the overlay; `Enter` saves, `Esc` cancels, and status shows `<token> / Group<N> → N ckts / M cables` in `modifier_hue`.

#### Scenario: Graph node edit
- **WHEN** graph is focused and `hovered_graph_node` is `motorfader:12` and user presses `e` then types and presses `Enter`
- **THEN** `circuits."motorfader:12"` updates, `labels.toml` is atomically rewritten, and both source header and graph node show the new title
