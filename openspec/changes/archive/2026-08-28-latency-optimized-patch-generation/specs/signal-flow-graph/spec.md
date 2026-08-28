# signal-flow-graph Specification (delta)

## ADDED Requirements

### Requirement: Latency optimizer menu (`g o`)

The graph surface MUST provide a `g o` key chord that generates latency-optimized candidate orderings (via the `latency-optimizer` capability) and opens a candidate menu overlay. With no patch loaded it MUST show a status hint instead.

- The menu lists up to 3 candidates, best first: variant label + `avg X→Y · max A→B · back-edges N→M` (before → after).
- `j`/`k` navigate; `Enter` previews the selected candidate in memory (graph recolors via the latency ramp); `s` exports it save-as; `r` restores the original order; `Esc` closes the menu (restoring the original order if a preview is active).
- While the menu is open it owns all keys (mirroring the validation-modal priority).
- The status line MUST show the active candidate label while a preview is loaded.

#### Scenario: Open menu with candidates

Given a loaded patch with cables and `g o` pressed, the menu shows the generated candidates with before/after summaries.

#### Scenario: No patch loaded

Given no patch and `g o` pressed, the status line shows a hint that no patch is loaded; no menu opens.

#### Scenario: Preview then Esc restores

Given a candidate previewed and `Esc` pressed, the menu closes and the patch returns to its original section order and coloring.

#### Scenario: Export from menu

Given a candidate selected and `s` pressed, the reordered patch is written save-as (see `patch-writing`), the source is untouched, and the status confirms the written path.