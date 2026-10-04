# Design

## Context

See proposal.md (Why). Current state: 40 capability specs under
`openspec/specs/` (feature inventory), active changes as the planned
column, help tables as the key reference, and a headless egui paint path
used by shape/label tests (the screenshot source). Audience: DROID
owners. Requests go to GitHub Issues.

## Goals / Non-Goals

**Goals:**

- A front door that motivates, disclaims loudly, shows, and onboards.
- Docs that stay true (generated screenshots, spec-sourced features).

**Non-Goals:**

- Behavior changes; videos; derived-doc edits.

## Decisions

- **README + `docs/`** (over README-only): front door plus manual scales;
  README links down, never duplicates.
- **Deterministic screenshots** (over hand captures): rendered by the
  headless paint path at fixed fixtures, so docs can't rot silently.
  Rationale: truthfulness beats beauty for v1.
- **Features curated from specs** (over exhaustive lists): the 40 specs
  are inventory, not prose; the README names what a DROID owner feels.
- **Loud disclaimer up top** (over fine-print): as-is, no responsibility,
  participation welcome — in the user's own voice, impossible to miss.
- **Onboarding in README + `docs/contributing.md`**: setup, submodule,
  4-gate, regen rule, OpenSpec + beads — the minimum viable contributor.

## Risks / Trade-offs

- [Risk] Screenshots rot when paint changes → Mitigation: harness runs on
  fixtures; staleness is a diff, not a surprise.
- [Risk] Feature list drifts from specs → Mitigation: curate from specs
  at write time; revisit per release, not per change.
