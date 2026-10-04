# Proposal

## Why

The project has no user-facing documentation (ARCHITECTURE.md §15:
"No README"). The app is built for DROID owners, but nothing tells them
what it is, what it does today, what's coming, or where to report the
pain-points that should shape it. A GitHub-style README plus a short user
manual closes that gap, alongside a developer onboarding so contributors
can build, verify, and extend the app without tribal knowledge.

## What Changes

- `README.md` (repo root): motivation, a LOUD as-is disclaimer (no
  responsibility whatever the software causes you to do, feel, or hear —
  participation welcome), screenshots, current features curated from the
  capability specs, planned work from the active changes, a pain-points
  feature-request query pointing at GitHub Issues, developer onboarding
  (setup + submodule, the 4-gate, doc regen rule, OpenSpec + beads
  workflow), build & run, and acknowledgements (OpenCode, OMLX, CortexAI,
  OpenSpec, Unsloth).
- `docs/` manual: getting-started, views, keys, labels, uploading, config,
  contributing (deep onboarding).
- A deterministic screenshot harness via the headless paint path, so
  screenshots stay truthful across changes.
- No spec deltas: nothing behavioral changes.

## Capabilities

### New Capabilities

- None (docs-only change).

## Impact

- Touched files: `README.md` (new), `docs/**` (new), screenshot tooling
  under `tools/` (new).
- No code, config, or spec changes. No ARCHITECTURE.md/DESIGN.md edits.

## Non-goals

- No behavior, keybinding, or theme changes.
- No video/demo GIFs (screenshots first; motion later).
- No hand-editing the derived ARCHITECTURE.md/DESIGN.md to match.

## Missing specialization (recorded per workflow)

- `.opencode/agents/` has no documentation specialist, so the two writing
  tasks (1.2, 1.3) carry a blank agent annotation. The orchestrator will
  need a docs-capable worker or direct execution for them.
