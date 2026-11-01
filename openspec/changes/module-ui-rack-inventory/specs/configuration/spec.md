# Spec Delta

## ADDED Requirements

### Requirement: Rack module inventory

The system SHALL load an optional `modules` list on each `[[physical.rack.row]]` entry of `config.toml`, naming the modules that row physically holds in left-to-right order. Each entry SHALL be a module key `"{controller} {instance}"` with a 1-based instance as the patch numbers it (`P2B8 1`, `E4 2`), or a bare controller name for a single-instance module (`B32`, `G8`, `X7`, `R2M`, `R2C`). The entries `master` and `CV I/O` SHALL both name the master slot and SHALL resolve to the master model resolved for the loaded patch, so one inventory describes either master. An entry naming a controller with no known geometry SHALL emit one warning on stderr and be skipped without affecting the remaining entries. An absent or empty `modules` list SHALL leave the previous behaviour: the chain is derived from the patch and rows are auto-packed.

A module listed in a row SHALL be placed in that row; `assign` SHALL NOT move it. `assign` SHALL keep applying to modules the inventory does not list.

#### Scenario: Inventory places modules in their rows

- **WHEN** `config.toml` declares two rows whose `modules` lists name four and three modules
- **THEN** the physical view shows those modules in list order, the first four in the first row and the next three in the second

#### Scenario: Absent inventory keeps today's rendering

- **WHEN** `config.toml` has no `modules` list on any row
- **THEN** the chain is derived from the loaded patch exactly as before and rows are auto-packed

#### Scenario: Unknown module entry warns and is skipped

- **WHEN** a row lists `"P3B7 1"`, a controller with no geometry entry
- **THEN** one warning naming the entry is printed and the row renders its other modules in order

#### Scenario: The master alias follows the patch

- **WHEN** a row lists `master` and the loaded patch declares `master=18`
- **THEN** that row slot renders the MASTER18 faceplate, not the MASTER face

#### Scenario: Instance-qualified keys bind patch components

- **WHEN** a row lists `P2B8 1` and `P2B8 2` and the patch addresses `B1.x` and `B2.x`
- **THEN** the `B1.x` components render on the first faceplate and the `B2.x` components on the second

#### Scenario: A listed module ignores assign

- **WHEN** `assign` maps a module to row 1 while a row's `modules` list names it in row 0
- **THEN** the module renders in row 0

#### Scenario: An unlisted module still honours assign

- **WHEN** a patch module is absent from every row's `modules` list and `assign` places it in row 1
- **THEN** the module renders in row 1
