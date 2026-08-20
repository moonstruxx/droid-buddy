# AGENTS.md

<!-- OB-NOT-INITIALIZED -->

# Agent operating guide

This guide defines the common operating contract for AI agents in this repository.
It is agent-agnostic and works with OpenCode, Claude Code, Codex, Gemini, and other agents.

## Purpose and scope

Use this file for repository-wide workflow rules. Keep product architecture, security constraints, and design rules in their source documents rather than duplicating them here.

## Session context

Before a non-trivial change, read these documents in order:

1. `AGENTS.md` for workflow and repository rules.
2. `ARCHITECTURE.md` for boundaries, dependencies, and component interactions.
3. `DESIGN.md` for UI and design-system work.
4. The active OpenSpec change or the relevant specification for the area being changed.

Read each document once per session unless it changes or the task moves into a different area.

Command aliases: OpenSpec skills may reference `/opsx-propose`, `/opsx-apply`, `/opsx-archive`, or `/opsx-explore`. Always substitute them with the `ob-plan-propose`, `ob-plan-apply`, `ob-plan-archive`, and `ob-plan-explore` skills respectively. User-facing command names are `/plan-propose`, `/plan-apply`, `/plan-archive`, and `/plan-explore`. Never mention the `opsx-` names to the user.

## Workflow ownership

<!-- OB-PLATFORM-WORKFLOW-START -->
This project uses a mixed-platform setup: the backlog (work items) and the repo (PRs) live on different platforms. Check `.opencode/opencode-onboard.json` → `platform.backlog` and `platform.repo`.

When the user provides a work item URL or says "implement the plan" or "I've added comments to the PR", **I own the full lifecycle**: parse the work item with `@ob-userstory` (backlog platform CLI), plan via the `ob-plan-propose` skill, confirm with the user, implement via the `ob-plan-apply` skill, ship via the `ob-ops-ship` skill (repo platform CLI).

Trigger patterns, I recognize ALL of these, exact wording does not matter:
- A backlog work-item URL or issue key → load `ob-userstory` → parse → load the `ob-plan-propose` skill → confirm with user → load the `ob-plan-apply` skill → ship
- `implement the plan` / `implement` / `start` / `go` → load the `ob-plan-apply` skill → ship
- A PR/MR URL or "I've added comments to the PR" → read the PR comments via the repo platform CLI → run the PR Feedback Loop

Never mix the CLIs: work items always go through the backlog platform CLI, PRs/MRs always through the repo platform CLI.
<!-- OB-PLATFORM-WORKFLOW-END -->

## Planning and execution

- Plan before delegating work. Use OpenSpec when the change needs explicit scope, decisions, or sequenced tasks.
- Keep changes focused. Do not combine unrelated refactors with requested work.
- Do not guess when requirements, architecture, or security constraints are unclear. Ask before proceeding.
- Prefer the project's established patterns and source documents over introducing new conventions.

## Engineer selection

Inspect `.opencode/agents/*.md` before spawning. Prefer the most specialized custom engineer. `fullstack-engineer` is `mode: primary`, the planning agent, and is not a spawned worker. If no specialist matches, tell the user to create one with `/make-engineer`. Spawn only engineers present in that directory.

Available engineers:
| `layout-designer-engineer` | `.opencode/agents/layout-designer-engineer.md` | Terminal UI layout, spacing, color, and component hierarchy for ratatui TUI |
| `rusty-engineer` | `.opencode/agents/rusty-engineer.md` | Rust application logic, monolith/layered architecture, and design patterns for the ratatui TUI |
| `horst-engineer` | `.opencode/agents/horst-engineer.md` | Rust unit and regression testing for the ratatui TUI |
| `dermannmitdermachine-engineer` | `.opencode/agents/dermannmitdermachine-engineer.md` | Rust application logic, DROID framework, monolith/layered architecture, and design patterns for the ratatui TUI |

The `ob-plan-apply` skill is authoritative for subagent waves, dependency ordering, retries, and concurrency. Read `agents.maxConcurrent` from `.opencode/opencode-onboard.json` before spawning workers.

## Tool and repository safety

- Never expose or commit secrets, credentials, tokens, or production data.
- Read before editing. Respect repository ownership, generated files, and existing local changes.
- Run only commands appropriate to the task. Do not bypass checks, weaken tests, or silence lint rules to get a green result.
- Commit, push, create pull requests, alter dependencies, or change deployment configuration only with the user's explicit approval and the repository's stated process.

## Verification and completion

- Run the applicable tests, lint, typecheck, and build before reporting completion.
- A bug fix needs a test that would have caught the defect when practical.
- Update specifications, architecture, or design documentation when the change makes their current statements inaccurate.
- Report changed files, checks run, and any remaining risk or follow-up work.

## Communication

- Keep updates concise and factual.
- State blockers early and explain the decision needed.
- Use the repository's language and writing conventions for source, documentation, issues, commits, and pull requests.
- Comments explain non-obvious reasons, constraints, or invariants. Do not add comments that restate code.

## Skills

Skills live in `.agents/skills/`. Always installed: `@ob-guardrails-generic`, `@ob-guardrails-project`, and `@browser-automation`. Agents load them via `@skill-name` in their `## Abilities` section.

<!-- OB-PLATFORM-SKILLS-GUIDE-START -->
Mixed platform setup: the two platform skills target different hosts:
- `@ob-userstory` → fetches work items from the backlog platform. Load when a work-item URL or issue key is provided.
- `ob-ops-ship` → creates PRs/MRs and triages review feedback on the repo platform. Load in ship mode or PR-feedback mode.
<!-- OB-PLATFORM-SKILLS-GUIDE-END -->

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
