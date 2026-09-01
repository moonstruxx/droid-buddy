# AGENTS.md

<!-- PC-NOT-INITIALIZED -->

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

Command aliases: OpenSpec skills may reference `/opsx-propose`, `/opsx-apply`, `/opsx-archive`, or `/opsx-explore`. Always substitute them with the `pc-plan-propose`, `pc-plan-apply`, `pc-plan-archive`, and `pc-plan-explore` skills respectively. User-facing command names are `/plan-propose`, `/plan-apply`, `/plan-archive`, and `/plan-explore`. Never mention the `opsx-` names to the user.

## Workflow ownership

<!-- PC-PLATFORM-WORKFLOW-START -->
This project uses a mixed-platform setup: the backlog (work items) and the repo (PRs) live on different platforms. Check `.opencode/harness.json` → `platform.backlog` and `platform.repo`.

When the user provides a work item URL or says "implement the plan" or "I've added comments to the PR", **I own the full lifecycle**: parse the work item with `@pc-userstory` (backlog platform CLI), plan via the `pc-plan-propose` skill, confirm with the user, implement via the `pc-plan-apply` skill, ship via the `pc-ops-ship` skill (repo platform CLI).

Trigger patterns, I recognize ALL of these, exact wording does not matter:
- A backlog work-item URL or issue key → load `pc-userstory` → parse → load the `pc-plan-propose` skill → confirm with user → load the `pc-plan-apply` skill → ship
- `implement the plan` / `implement` / `start` / `go` → load the `pc-plan-apply` skill → ship
- A PR/MR URL or "I've added comments to the PR" → read the PR comments via the repo platform CLI → run the PR Feedback Loop

Never mix the CLIs: work items always go through the backlog platform CLI, PRs/MRs always through the repo platform CLI.
<!-- PC-PLATFORM-WORKFLOW-END -->

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
| `api-engineer` | `.opencode/agents/api-engineer.md` | API/Integration, ComfyUI-style node graphs, force-directed physics layout, and third-party integration for the ratatui TUI |

The `pc-plan-apply` skill is authoritative for subagent waves, dependency ordering, retries, and concurrency. Read `agents.maxConcurrent` from `.opencode/harness.json` before spawning workers.

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

Skills live in `.agents/skills/`. Always installed: `@pc-guardrails-generic`, `@pc-guardrails-project`, and `@browser-automation`. Agents load them via `@skill-name` in their `## Abilities` section.

<!-- PC-PLATFORM-SKILLS-GUIDE-START -->
Mixed platform setup: the two platform skills target different hosts:
- `@pc-userstory` → fetches work items from the backlog platform. Load when a work-item URL or issue key is provided.
- `pc-ops-ship` → creates PRs/MRs and triages review feedback on the repo platform. Load in ship mode or PR-feedback mode.
<!-- PC-PLATFORM-SKILLS-GUIDE-END -->

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

<!-- BEGIN BEADS INTEGRATION v:1 profile:full hash:f2c52d34 -->
## Issue Tracking with bd (beads)

**IMPORTANT**: This project uses **bd (beads)** for ALL issue tracking. Do NOT use markdown TODOs, task lists, or other tracking methods.

### Why bd?

- Dependency-aware: Track blockers and relationships between issues
- Git-friendly: Dolt-powered version control with native sync
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion

### Quick Start

**Check for ready work:**

```bash
bd ready --json
```

**Create new issues:**

```bash
bd create "Issue title" --description="Detailed context" -t bug|feature|task -p 0-4 --json
bd create "Issue title" --description="What this issue is about" -p 1 --deps discovered-from:bd-123 --json
```

**Claim and update:**

```bash
bd update <id> --claim --json
bd update bd-42 --priority 1 --json
```

**Complete work:**

```bash
bd close bd-42 --reason "Completed" --json
```

### Issue Types

- `bug` - Something broken
- `feature` - New functionality
- `task` - Work item (tests, docs, refactoring)
- `epic` - Large feature with subtasks
- `chore` - Maintenance (dependencies, tooling)

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Workflow for AI Agents

1. **Check ready work**: `bd ready` shows unblocked issues
2. **Claim your task atomically**: `bd update <id> --claim`
3. **Work on it**: Implement, test, document
4. **Discover new work?** Create linked issue:
   - `bd create "Found bug" --description="Details about what was found" -p 1 --deps discovered-from:<parent-id>`
5. **Complete**: `bd close <id> --reason "Done"`

### Quality
- Use `--acceptance` and `--design` fields when creating issues
- Use `--validate` to check description completeness

### Lifecycle
- `bd defer <id>` / `bd supersede <id>` for issue management
- `bd stale` / `bd orphans` / `bd lint` for hygiene
- `bd human <id>` to flag for human decisions
- `bd formula list` / `bd mol pour <name>` for structured workflows

### Sync

bd stores issue history in Dolt:

- Each write auto-commits to Dolt history
- Do not treat `.beads/issues.jsonl` as the sync protocol

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ✅ Check `bd ready` before asking "what should I work on?"
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers
- ❌ Do NOT duplicate tracking systems

For more details, see README.md and docs/QUICKSTART.md.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.

<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->
## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
