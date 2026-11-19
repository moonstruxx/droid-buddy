# Keys

The in-app `?` modal shows the key table for the focused view — it is
the authority if this page ever disagrees. `g` arms a prefix for 1 s;
the chords are `g v` (source viewer), `g g` (graph), `g d` (diff
picker), `g o` (optimizer), `g s` (select-state menu), `g c` (latency
coloring), `g f` (rack fit).

## Module UI

| Key | Action |
|---|---|
| `j`/`k` | navigate |
| `click` | focus element across source + graph |
| `Enter`/`Space` | toggle component |
| `+`/`-` | zoom presets |
| `Shift+F` | fit written labels, each in its cell (rack may pan) |
| `g f` | fit whole rack to pane (overview) |
| `f` / `z` | maximize focused pane |
| `c` / `Shift+C` | centre focused element (else rack) / fit written labels and re-arm auto-fit |
| `click minimap` | centre the pane on the clicked rack point |
| `drag minimap` | fit the zoom to the dragged region |
| `a` | cycle label presentation |
| `arrows` | pan rack on overflow |
| `wheel` | adjust knob/fader, else zoom about pointer |
| `s` | toggle skeleton presentation |
| `m` | latch modifier on hovered component |
| `e` | edit label / validation modal |
| `1`–`4` | shift groups |
| `d` | toggle diff overlay |
| `p` / `R` | open performance view / reset element states |
| `U` / `y` / `n` | upload confirm / send / cancel |

## Performance view (`p`)

`p` on the Module UI pane opens the exploded-label presentation. Its zoom
and label-scale keys are read only while that view is showing in the
focused pane.

| Key | Action |
|---|---|
| `+`/`-` | zoom the rack and its callouts (compact quarter of the pane up to the whole pane) |
| `wheel` | same zoom (the compact rack stays centred) |
| `Shift`+`+` / `Shift`+`-` | step the callout base scale by 5 % (50–400 %) |
| `R` / `Esc` | reset element states / leave the view |

How the shifted pair arrives depends on the layout, because the key is
bound by the glyph it produces. `Shift+-` produces `_` on both layouts,
so the down step is one key everywhere. `Shift++` produces `*` where `+`
is itself unshifted (German) and `+` carrying the shift flag where it is
not (US); both forms are accepted, but on the second kind of layout the
plain `+` press is therefore the label step rather than the zoom step,
and the wheel is what zooms the rack up. Outside this view the pair
keeps its other meaning: on the graph pane it scales the other pane's
zoom, and on any other pane it does nothing.

## Source viewer

| Key | Action |
|---|---|
| `j`/`k` | scroll source |
| `wheel` | scroll source (pointer over the source column) |
| `Up`/`Down`, `Home`/`End` | navigate occurrences |
| `t` | toggle raw/prettified |
| `[`/`]` | adjust pane split |
| `e` | edit label |
| `Esc` | close viewer |

## Signal-flow graph

| Key | Action |
|---|---|
| `x` | toggle circuit processing |
| `p` | pin/unpin node |
| `c` / `Shift+c` | center graph / fit and center |
| `h` | toggle column/force layout |
| `a` | apply arrangement / cycle layouts |
| `Shift+F` / `i` | dependency filter / influence filter |
| `o` | isolate group |
| `Enter` | expand/collapse group |
| `Shift+O` | toggle grouping |
| `+`/`-`, `arrows` | camera zoom / pan |
| `click minimap` | pan camera to clicked position |
| `drag minimap` | zoom to the dragged region |
| `Alt+[`/`Alt+]` | cable tension (force solver) |
| `e` / `d` | edit label / diff overlay |
| `Esc` | close graph |

## Optimizer

| Key | Action |
|---|---|
| `j`/`k` | navigate candidates |
| `Enter` (or row click) | preview |
| `r` | restore original order |
| `s` | export to `<stem>-latopt.ini` |
| `[`/`]` | adjust weight |
| `0`/`1` | snap weight |
| `Esc` | close |

## Validation modal

| Key | Action |
|---|---|
| `j`/`k` | navigate issues |
| `Enter` | jump to source |
| `e` / `Esc` | close |

## File picker

| Key | Action |
|---|---|
| `j`/`k`/`arrows` | navigate |
| `Enter` | select |
| `type` / `Backspace` | filter entries |
| `Ctrl+f` / `q` | toggle filter / end filter |
| `f`/`F` | toggle favourite |
| `0`–`9` | fast-select favourite slot |
| `Esc` | close |

## Pane management (any pane view)

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | cycle pane focus |
| `r` | cycle view in focused pane (optimizer: restore) |
| `z` / `f` | maximize focused pane (non-latching — again, or `Tab`/`Esc`, restores) |
| `Alt+b` / `Alt+s` | swap big panes / swap small panes |
| `Esc` | close focused view |

## Global

| Key | Action |
|---|---|
| `l` | open file picker |
| `g` | prefix mode |
| `?` | help modal |
| `p` | pause processing (everywhere except graph/performance panes) |
| `q` / `Ctrl+c` | quit |

See [Views](views.md) for what each surface does.
