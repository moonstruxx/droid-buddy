# Keys

The in-app `?` modal shows the key table for the focused view — it is
the authority if this page ever disagrees. `g` arms a prefix for 1 s;
the chords are `g v` (source viewer), `g g` (graph), `g d` (diff
picker), `g o` (optimizer), `g s` (select-state menu), `g c` (latency
coloring).

## Module UI

| Key | Action |
|---|---|
| `j`/`k` | navigate |
| `click` | focus element across source + graph |
| `Enter`/`Space` | toggle component |
| `+`/`-` | zoom presets |
| `arrows`/`wheel` | pan rack on overflow |
| `s` | toggle skeleton presentation |
| `m` | latch modifier on hovered component |
| `e` | edit label / validation modal |
| `1`–`4` | shift groups |
| `d` | toggle diff overlay |
| `p` / `R` | open performance view / reset element states |
| `U` / `y` / `n` | upload confirm / send / cancel |

## Source viewer

| Key | Action |
|---|---|
| `j`/`k` | scroll source |
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
| `f` / `i` | dependency filter / influence filter |
| `+`/`-`, `arrows` | camera zoom / pan |
| `click minimap` | pan camera to clicked position |
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
| `z` | maximize focused pane |
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
