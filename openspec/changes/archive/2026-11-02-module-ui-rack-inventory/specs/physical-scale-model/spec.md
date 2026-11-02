# Spec Delta

## MODIFIED Requirements

### Requirement: Physical grid model

The system SHALL model a patch's physical layout as an ordered chain of controller modules with real millimeter geometry — module width derived from HP (1 HP = 5.08 mm), element cells with position and size per element family — derived from embedded per-controller geometry data. The chain SHALL begin with exactly one master module, at chain position 0, whose model is the master model resolved for the patch: MASTER or MASTER18, never both and never neither. The master module's geometry SHALL come from that model's embedded entry, and the module SHALL keep the `CV I/O` controller identity so rack assignments and the input/output jack cells continue to resolve as they do today.

When no rack module inventory is configured, the chain SHALL be the order the patch declares its controllers. When an inventory is configured, the chain SHALL be that inventory in row order and, within a row, in list order, followed by every patch module the inventory does not name, in patch declaration order. A module the patch does not address SHALL still take part in the chain, with an empty component list, and SHALL keep its instance identity from the inventory key so later components of that instance attach to it. The chain SHALL be buildable without a patch, in which case every module has an empty component list.

#### Scenario: Chain order matches patch declaration

- **WHEN** a patch declares controllers in a given order and no inventory is configured
- **THEN** the physical layout places their modules left-to-right in that order.

#### Scenario: Chain order matches the inventory

- **WHEN** an inventory lists controllers in a different order than the patch declares them
- **THEN** the physical layout follows the inventory and appends the remaining patch modules after it.

#### Scenario: Repeated instances become separate faceplates

- **WHEN** a patch declares two instances of the same controller
- **THEN** each instance is rendered as its own module at its real width, placed in declaration order.

#### Scenario: Element cells resolve per token

- **WHEN** a hardware token (B/P/S/E/I/O/L family) belongs to a controller
- **THEN** its element cell position and size resolve from that controller's geometry data.

#### Scenario: Every chain has exactly one master, first

- **WHEN** any patch is laid out, including one that addresses no master jack at all
- **THEN** the first module is a master module and no other module is one

#### Scenario: The master model follows the patch

- **WHEN** a patch declares `master=18`
- **THEN** the first module uses the MASTER18 geometry (6 HP, `O1..O8` plus the gate jacks), not the MASTER geometry

#### Scenario: An unaddressed inventory module still occupies its slot

- **WHEN** an inventory lists a `G8 1` that the loaded patch never addresses
- **THEN** a G8 faceplate renders at that chain position with every declared cell and no components

#### Scenario: A patch module missing from the inventory is appended

- **WHEN** the inventory lists `P2B8 1` and the patch also declares a `B32 1`
- **THEN** the B32 faceplate renders after the inventory modules

#### Scenario: A chain is built without a patch

- **WHEN** an inventory is configured and no patch is loaded
- **THEN** the chain contains exactly the inventory modules, each with no components, and the master module follows the default master model

### Requirement: Rack model

The system SHALL model the case/rack as an ordered set of rows — each with a height (HE: 1 or 3), a width (HP), an optional label, and an optional list of the modules the row physically holds — plus optional top-mount and side-mount sections (in TE). A module named by a row's module list SHALL be placed in that row in list order. Every module the inventory does not name SHALL be assigned to rows by auto-packing in chain order, overridable per module.

#### Scenario: Auto-pack fills rows in chain order

- **WHEN** a rack has rows, a chain of controllers, and no module inventory
- **THEN** controllers pack left-to-right into row 0 until the next module would exceed the row's HP, which then starts row 1, and so on.

#### Scenario: Per-module override

- **WHEN** the config assigns a module to a specific row
- **THEN** the module is placed in that row regardless of auto-pack; an out-of-range override falls back to auto-pack.

#### Scenario: Inventory rows hold their listed modules

- **WHEN** a row's module list names three modules and a later row names two
- **THEN** the packer places exactly those modules in those rows, in list order, without auto-packing them

#### Scenario: User-defined case

- **WHEN** the user configures `[physical.rack]` with rows and TE mounts
- **THEN** the physical view renders the case structure (rows, top/side mount regions) with the modules inside.

#### Scenario: Default case

- **WHEN** no rack config is given
- **THEN** a single row wide enough for the whole chain is used.

## ADDED Requirements

### Requirement: Element identity in geometry data

Every element cell in the embedded per-controller geometry SHALL carry an element identity: an element number and, for a family whose faceplate shows several jack kinds at once, a kind letter. The identity SHALL be derived from the cell's label when the label names a hardware token (`B1.3` → element 3, `I5` → (`I`, 5)) or given explicitly in the geometry data when the label does not. A cell with no identity SHALL NOT render and SHALL NOT resolve a token.

MASTER18's gate column SHALL carry the token names the patch language uses: `I1` and `I2` for its two logic inputs and `G1..G4` for its four gate outputs. The R2M/R2C serial jacks SHALL carry explicit element numbers while remaining non-addressable, because no patch token reaches them.

#### Scenario: MASTER18 logic input resolves to its own jack

- **WHEN** a patch addressing `I1` renders on MASTER18
- **THEN** the marker is drawn on the `I1` cell of the gate column, not on `O1`

#### Scenario: MASTER18 gate outputs resolve

- **WHEN** a patch addressing `G1..G4` renders on MASTER18
- **THEN** each marker is drawn on its `G1..G4` cell

#### Scenario: An expander exposes its jacks

- **WHEN** an R2C faceplate renders
- **THEN** both of its declared CV cells are rendered

#### Scenario: A cell without identity stays invisible

- **WHEN** a geometry cell has neither a token-shaped label nor an explicit element number
- **THEN** it renders no cell and no token resolves to it
