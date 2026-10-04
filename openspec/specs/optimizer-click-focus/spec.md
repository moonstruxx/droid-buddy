# optimizer-click-focus Specification

## Purpose

Clicking (or pressing `Enter` on) a latency-optimizer candidate previews its section reorder and focuses the affected circuit across all views: the signal-flow graph recolors by latency delta against the file-order baseline, the source viewer jumps to the first affected circuit, and the module UI highlights it. The optimizer pane stays open and keeps focus throughout.

## Requirements

### Requirement: Click previews the reorder

Clicking a candidate row MUST behave exactly like pressing `Enter` on it: the candidate's section order is applied, the graph rebuilds, the camera refits so the new layout is visible, and the status line reports the preview. The optimizer pane MUST remain open, and clicking another candidate MUST move the preview to that candidate.

#### Scenario: Click equals Enter

Given the optimizer open with candidates, clicking a row produces the same reordered patch, rebuilt graph, and status message as pressing `Enter` on the focused row.

#### Scenario: Re-click moves the preview

Given a preview active on one candidate, clicking a different row replaces the preview with the new candidate's order and recolor.

### Requirement: Latency-delta graph recolor

While a preview is active, the graph MUST recolor each edge against the file-order baseline captured when the optimizer opened: edges whose forward-loop latency got worse render red, edges that got better render blue, unchanged edges keep their normal color. Node borders MUST aggregate the direction of their incident edges (worse beats better); unchanged nodes keep their resolved border. The recolor sits under the existing precedence: topology-error highlighting, dimming (disabled / not-selected / uninfluenced), and influence highlighting all win over it.

#### Scenario: Worse and better edges distinguished

Given a preview that raises one cable's latency and lowers another's, the graph shows the first cable red and the second blue, with the affected node borders matching the direction of their edges.

#### Scenario: Unchanged preview keeps single-state render

Given a preview whose order produces no latency change, the graph renders with its normal coloring and no focus jump occurs.

### Requirement: Cross-view focus on the first affected circuit

On preview, the application MUST focus the sink of the first edge (in file order) whose latency moved beyond epsilon: the source viewer scrolls to that circuit's first occurrence and the module UI highlights the corresponding hardware elements through the existing shared-selection machinery. Focus MUST be handed back to the optimizer pane afterwards, so candidate navigation, restore, and export keep working.

#### Scenario: Source and module UI follow the preview

Given a preview that changes at least one edge's latency, the source viewer shows the first affected circuit's occurrence and the module UI highlights its elements, while keyboard focus stays on the optimizer pane.

### Requirement: Preview baseline lifecycle

The file-order per-edge latency baseline MUST be captured when the optimizer opens and cleared when the preview is restored, when the optimizer closes, and when a new patch loads, so a later preview always compares against the current file order.

#### Scenario: Baseline cleared on restore

Given a preview active, restoring the original order clears the baseline and returns the graph to single-state rendering.

### Requirement: Restore on demand

Restoring (`r` on the optimizer pane) MUST return the patch to its original section order, rebuild the graph when a preview was active, and report the restore. Closing the optimizer (`Esc`) MUST drop the preview and the baseline along with the pane.

#### Scenario: Restore returns to file order

Given a preview active, restoring yields the original section order, the pre-preview graph coloring, and no remaining diff state.
