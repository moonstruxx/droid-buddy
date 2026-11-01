# Spec Delta

## MODIFIED Requirements

### Requirement: Physical grid model

The system SHALL model a patch's physical layout as an ordered chain of controller modules with real millimeter geometry — module width derived from HP (1 HP = 5.08 mm), element cells with position and size per element family — derived from embedded per-controller geometry data, in the order the patch declares the controllers. The chain SHALL begin with exactly one master module, at chain position 0, whose model is the master model resolved for the patch: MASTER or MASTER18, never both and never neither. The master module's geometry SHALL come from that model's embedded entry, and the module SHALL keep the `CV I/O` controller identity so rack assignments and the input/output jack cells continue to resolve as they do today.

#### Scenario: Chain order matches patch declaration

- **WHEN** a patch declares controllers in a given order
- **THEN** the physical layout places their modules left-to-right in that order.

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
