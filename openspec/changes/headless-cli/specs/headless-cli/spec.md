# Spec Delta

## Purpose

Exposes the app's patch analysis to scripts, CI and AI agents without a window: a
command surface that reports validation, master fit, RAM, topology and latency for a
patch, proposes section reorderings, and returns machine-parseable results with exit
codes instead of pixels.

## ADDED Requirements

### Requirement: Headless analysis command

The binary SHALL accept `analyze <patch>` where `<patch>` is a path to an `.ini` file or
`-` for standard input, and SHALL produce one analysis report on standard output without
opening a window, allocating a GPU device, or reading the display. A subcommand SHALL take
precedence over a positional patch path, and `--` SHALL force the following argument to be
read as a path even when it spells a subcommand name. Invoking the binary with a path and
no subcommand SHALL keep opening the window as before.

#### Scenario: Analyzing a file

- **WHEN** the binary runs `analyze fixtures/arpeggio1.ini`
- **THEN** it prints one report for that patch and exits 0

#### Scenario: Analyzing standard input

- **WHEN** the binary runs `analyze -` with a patch on standard input
- **THEN** it reports that patch exactly as if it had been read from a file

#### Scenario: A subcommand wins over a same-named file

- **WHEN** the current directory contains a file named `analyze` and the binary runs `analyze analyze`
- **THEN** the first argument is treated as the subcommand and the second as the patch path

#### Scenario: A path may name a subcommand with the escape

- **WHEN** the binary runs `analyze -- optimize`
- **THEN** `optimize` is read as a patch path

#### Scenario: No subcommand keeps the window

- **WHEN** the binary runs with one argument that is a path and no subcommand
- **THEN** it opens the window with that patch, unchanged from before this capability

### Requirement: Analysis report content

The report SHALL contain, for one patch: a patch summary (path or `<stdin>`, name, byte
size, section count, circuit count, cable count, hardware-token count); a schema summary
(circuit count and where the schema came from); a master summary (resolved model, whether
it was declared, inferred or defaulted, and whether any model can host the patch); RAM
used and the budget of the resolved model; the validation findings; the topology
findings; the latency summary; and the banner groups.

Every validation finding SHALL carry its severity, its code, its message and a source
location as a 1-based line with 0-based start and end columns. Findings SHALL appear in
source order. Topology findings SHALL carry the cable they concern, their severity and
their message. The latency summary SHALL contain the average and maximum per-edge
forward-loop latency and the back-edge count, plus a bounded list of the highest-latency
edges, each naming its cable and its source and sink circuits. Banner groups SHALL carry
their title and their section range. Latency values SHALL be reported with at most four
decimal places.

#### Scenario: Validation findings carry a source location

- **WHEN** a patch has a duplicate parameter on line 12
- **THEN** the report contains that finding with line 12 and its parameter-value column range

#### Scenario: Findings are ordered by source position

- **WHEN** a patch produces findings on lines 12, 3 and 40
- **THEN** the report lists them in the order 3, 12, 40

#### Scenario: The master verdict names its source

- **WHEN** a patch declares `master=18` in its leading comment block
- **THEN** the report names MASTER18 with the source `declared`

#### Scenario: RAM is judged against the resolved model

- **WHEN** a patch resolves to MASTER and is inside MASTER's budget but above MASTER18's
- **THEN** it reports the usage and MASTER's budget, and reports no RAM error

#### Scenario: Latency names the worst edges

- **WHEN** a patch has more cables than the worst-edge list can hold
- **THEN** the list holds only the highest-latency ones, ordered from worst down, each naming its cable and endpoints

### Requirement: Report determinism and reproducibility

Running the same command twice on the same patch with the same schema SHALL produce
byte-identical standard output. The report SHALL record the schema's provenance — that it
was the embedded schema, or that plugin files were overlaid, naming each applied file — so
two differing reports for one patch are explainable. A `--no-plugins` option SHALL
restrict the run to the embedded schema.

#### Scenario: Repeated runs are identical

- **WHEN** the same analysis runs twice on one patch
- **THEN** both standard outputs are byte-identical

#### Scenario: Plugin overlay is visible in the report

- **WHEN** a plugin file defines a circuit the patch uses and plugins are enabled
- **THEN** the patch validates clean and the report names that plugin file as applied

#### Scenario: Plugins can be excluded

- **WHEN** the same patch is analyzed with `--no-plugins`
- **THEN** the report names the embedded schema and the plugin circuit is reported as unknown

### Requirement: Optimization command

The binary SHALL accept `optimize <patch>` with the same patch-input rules as analysis,
and SHALL report the current file-order latency and every candidate reordering with its
latency after reordering and the difference from the baseline. It SHALL accept a strategy
selection (banner-preserving, global or all strategies including the min-max objective), an
objective weight in `[0, 1]` that trades total latency against the worst edge, and a
candidate index that selects which candidate an output option acts on.

`--write <path>` SHALL save the selected candidate as a patch file without modifying the
source, and SHALL report the path actually written. An existing destination SHALL NOT be
overwritten; a suffixed name SHALL be used instead, and the report SHALL name the suffixed
path. `--stdout` SHALL instead emit the selected candidate's patch text on standard output.

#### Scenario: The baseline and candidates are reported

- **WHEN** a patch with reorderable sections is optimized
- **THEN** the report contains the current latency and at least one candidate with its after-latency and delta

#### Scenario: Save-as reports the path actually written

- **WHEN** `optimize p.ini --write p-latopt.ini` runs while that file already exists
- **THEN** a suffixed file is written and the report names the suffixed path

#### Scenario: The source is never overwritten

- **WHEN** `optimize p.ini --write p.ini` runs
- **THEN** the command fails with a message naming the source, and the source is unchanged

#### Scenario: Emitted patch parses back

- **WHEN** `optimize p.ini --stdout` runs
- **THEN** the emitted text parses as a patch with the same sections in the reordered sequence

#### Scenario: Weights and strategies are accepted

- **WHEN** `optimize p.ini --strategy minmax --weight 0.6` runs
- **THEN** the candidates are scored under that weighted objective and the report says so

#### Scenario: Standard input cannot be written back

- **WHEN** `optimize - --write out.ini` runs with a patch on standard input
- **THEN** the command fails with a message explaining that a path is required, and writes no file

### Requirement: Exit codes and stream discipline

The binary SHALL exit `0` when the command completed and no finding reached the deny
threshold, `1` when at least one finding did, and `2` when the command could not run at
all — unknown or incomplete arguments, an unreadable file, or a patch that does not
parse. Standard output SHALL carry only the report (or the emitted patch for `--stdout`);
diagnostics, warnings and error messages SHALL go to standard error, and standard output
SHALL be empty when the command exits `2`. The deny threshold SHALL default to error
severity and SHALL be raisable to warnings.

#### Scenario: A clean patch exits zero

- **WHEN** a patch with no errors is analyzed
- **THEN** the exit code is 0

#### Scenario: Error findings fail the command

- **WHEN** a patch with an error-severity finding is analyzed
- **THEN** the report is still printed and the exit code is 1

#### Scenario: Raising the threshold fails on warnings

- **WHEN** a patch with only warning findings is analyzed with `--deny warning`
- **THEN** the exit code is 1

#### Scenario: An unreadable path fails cleanly

- **WHEN** the command names a file that does not exist
- **THEN** the exit code is 2, standard output is empty, and the reason is on standard error

#### Scenario: An unparsable patch fails cleanly

- **WHEN** the command reads a file that is not a DROID patch
- **THEN** the exit code is 2, standard output is empty, and the parse error is on standard error

### Requirement: Agent skill documentation

The project SHALL ship an agent skill for this command surface that documents every
command and option, the exit-code contract, and every top-level field of the analysis
report, with worked flows for gating a patch, diagnosing a master misfit, locating the
worst-latency cables, optimizing and saving a reordering, and piping a patch through
standard input. A test SHALL fail when a top-level report field is not documented in the
skill's report reference.

#### Scenario: The skill covers the report contract

- **WHEN** the report gains a top-level field that the skill's report reference does not document
- **THEN** the test suite fails naming the undocumented field

#### Scenario: The skill describes each command

- **WHEN** an agent reads the skill
- **THEN** it finds both commands, their options, the exit codes and the link to the user manual page
