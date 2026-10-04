# Views

## The pane layout

The main band is a class-based layout: a full-height left **big**
pane plus a right half that is either one big pane (no small view
open) or two stacked **small** panes. Every view belongs to a class:

- Big: Module UI, signal-flow graph.
- Small: source viewer, latency optimizer.

`Tab` / `Shift+Tab` cycle focus across the open panes in tree order,
`r` rotates the focused pane through its class carousel
(big: graph → Module UI; small: source viewer → optimizer), `z`
maximizes the focused pane, `Alt+b` / `Alt+s` swap same-class panes,
`Esc` closes the focused view. `[` / `]` move the left/right split
(30–70 %), `Alt+[` / `Alt+]` move the small-pane split.

## Module UI

The physical faceplate view — the app's only hardware surface. Every
element of a declared controller renders at its faceplate position;
unused elements are drawn dimmed and unlabelled. LEDs fold into their
element and render the patch's LED color/brightness.

- `j`/`k` and arrows navigate, `Enter`/`Space` toggles the hovered
  component, the mouse wheel adjusts knobs/faders.
- Click an element to **focus** it across surfaces: the cell stays
  highlighted, the source viewer jumps to its occurrence, and the
  graph highlights the element's register entity plus its influence
  subtree (centering the camera when the graph is open).
- `1`–`4` select a shift group (matching panels get bold colored
  borders, the rest dim; `Esc` clears).
- Hold or latch a modifier to wash the influenced cells: mouse-down on
  a modifier-eligible cell holds momentarily; `m` on the hovered
  component (or `Ctrl+Shift+Click`) latches until `Esc`.
- `+`/`-` cycle zoom presets (75–200 %), arrows/wheel pan when the
  rack overflows, `s` toggles the skeleton reference presentation,
  `p` opens the [performance view](#performance-view), `R` resets all
  element states, `e` edits the label (or opens validation).
  ![Module UI](assets/module-ui.png)

## Source viewer (`g v`)

Readonly `.ini` view in a small pane. Raw lines by default, `t`
switches to prettified circuit blocks. `j`/`k` scroll, `Up`/`Down` /
`Home`/`End` navigate the selected token's occurrences. Selection,
shift, and modifier highlights mirror the Module UI. `Esc` closes the
pane, keeping selection and scroll.
![Source viewer](assets/source-viewer.png)

## Signal-flow graph (`g g`)

Circuits as nodes, virtual `_cable` connections as directed edges,
banner groups as cluster containers — laid out by the deterministic
column arrangement by default (`h` toggles the retained force
solver, `a` applies the active arrangement and cycles the layout).
![Signal-flow graph](assets/graph.png)

- Drag a node to re-settle its neighborhood; `x` toggles processing
  for the hovered circuit; `p` pins/unpins the hovered node.
- `c` centers the graph, `Shift+c` refits and centers; `+`/`-` zoom,
  arrows pan, clicking the minimap pans to the clicked point.
- `f` filters to the hovered node's upstream-dependency subgraph
  (`Esc` clears the filter first); `i` toggles the influence filter.
- `g s` opens the select-state menu (assume `select` values; sections
  reclassify to Selected/NotSelected/Unknown); `g c` toggles cable
  latency coloring; `d` toggles the [diff overlay](#patch-diff-g-d-then-d).
- `Alt+[` / `Alt+]` adjust cable tension (force solver only).
- `Esc` closes the graph pane.

## Latency optimizer (`g o`)

A small pane proposing section reorderings that reduce forward-loop
latency. `j`/`k` move between candidates, `[`/`]` adjust the
objective weight `w` (`0`/`1` snap it), `Enter` or a row click
previews (reorders, rebuilds, recolors latency-worse red /
latency-better blue, focuses the first affected circuit), `s` exports
to `<stem>-latopt.ini`, `r`/`Esc` restore and close.
![Latency optimizer](assets/optimizer.png)

## File picker (`l`)

Lists the current directory; `j`/`k`/arrows navigate, `Enter` selects,
typing filters (`Ctrl+f` toggles the filter, `Backspace` edits,
`q` ends it), `f`/`F` toggles a favourite, `0`–`9` fast-select a
favourite slot, `Esc` closes.

## Validation modal (`e`)

Lists schema and lint findings sorted by source position with severity
badges (E/W/H). `j`/`k` navigate, `Enter` jumps the source viewer to
the issue's `line:col`, `Esc`/`e` closes. Any `Error`-severity finding
gates the load (the patch is rejected); warnings and hints load with
a `press 'e' to view` status hint.

## Patch diff (`g d`, then `d`)

`g d` opens the picker to choose the second (B) patch; the diff
computes once. `d` toggles the overlay, `Esc` clears the diff scope.
The graph colors added/removed/changed cables and nodes; the status
bar shows `Diff scope: <token> (N cables)`.

## Performance view

`p` on the Module UI pane opens an exploded-label presentation:
a compact centered rack with big side-bound callouts and leader
lines. Activating an element updates its callout live, `R` resets all
element states, `Esc` leaves the view.

## Select-state menu (`g s`)

Lists the patch's discovered Register/Cable select signals with
inferred candidate values. `j`/`k` move, `[`/`]`/`Enter` cycle the
value (rebuilding the graph each step), `Esc` clears and restores the
default graph. The status reports
`Select state: N selected / M unselected / K unknown`.
