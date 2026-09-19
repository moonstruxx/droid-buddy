## ADDED Requirements

### Requirement: Graph centering and fit keys

The graph surface SHALL bind `c` to center the graph (pan so the drawn content's center lands at the visible pane's center, keeping the current zoom) and `Shift+c` to fit-and-center (reframe the whole graph against the visible pane). The latency coloring toggle SHALL move from bare `c` to the `g c` chord. Bare `c` on surfaces other than the graph SHALL keep its existing meaning, if any.

#### Scenario: c centers the graph

- **WHEN** the graph surface is open and focused and the user presses `c`
- **THEN** the graph camera pans to center the drawn content in the visible pane and the zoom is unchanged

#### Scenario: Shift+c fits and centers the graph

- **WHEN** the graph surface is open and focused and the user presses `Shift+c`
- **THEN** the graph camera refits to frame the whole graph in the visible pane, centered

#### Scenario: g c toggles latency coloring

- **WHEN** the graph surface is open and the user presses `g` then `c`
- **THEN** latency coloring toggles and the status line reports the new state

#### Scenario: Non-matching key while prefix armed

- **WHEN** the g-prefix is armed and the user presses a key other than the known chords
- **THEN** the prefix is cleared and the key processes as a normal key event, so `g c` is the only way bare `c` toggles latency coloring while the chord list includes it