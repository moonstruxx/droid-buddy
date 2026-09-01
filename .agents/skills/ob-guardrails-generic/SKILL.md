---
name: ob-guardrails-generic
description: Generic guardrails, foundational rules that all agents follow. Users add specialized guardrails skills for specific concerns. Covers secrets, code quality, security, tool usage, and engineer workflow.
license: MIT
---

## Transitive loads (optimization skills)

The marker sections below may contain instructions for selected optimization skills. These are mandatory. If a section says "call `skill("xxx")`", you must call the skill tool with that exact name before doing any work.

## Secrets

- Treat `.env` files as write-only: write to them when configuring, read credentials from the environment or secret store at runtime.
- Keep credentials, API keys, and tokens out of logs and output.
- Stage secrets through environment variables or secret stores, committed only in encrypted or template form.

## Code

- Run tests before marking done.
- Run lint/build before pushing.
- Keep changes small and focused.
- Comments are for WHY, not WHAT. Use them only when the code does something non-obvious or the reason cannot be inferred from context. Keep comment ratio under 10%. If more than 10% of lines in a file are comments, refactor for clarity instead.
- Each file should have one clear responsibility. Split by domain or feature (e.g. `user-constants.ts`, `order-types.ts`, `auth-config.ts`) rather than creating catch-all files like `constants.js`, `types.ts`, `config.js`, or `utils.ts` that collect unrelated things. A file that imports from many unrelated modules is a sign it should be split.

## Temporary files

- Create scratch files only under `$REPO_ROOT/.opencode/.tmp/`; create a task-specific child directory when needed.
- Keep final artifacts in their required repository path. Copy or move a scratch artifact into that path before reporting it.
- Never use operating-system temporary directories or paths outside `$REPO_ROOT`.
- Remove scratch files when the task ends unless they are needed to diagnose a failure.

## Security

- Validate all inputs.
- Escape all outputs.
- Keep credentials in environment variables or secret stores, committed only in encrypted or template form.

## Communication

- Ask for clarification if unclear.
- Report blockers immediately.
- Show progress when asked.

<!-- OB-GUARDRAILS-CODEGRAPH-START -->

<!-- OB-GUARDRAILS-CODEGRAPH-END -->

<!-- OB-GUARDRAILS-MEMORY-START -->

<!-- OB-GUARDRAILS-MEMORY-END -->

<!-- OB-GUARDRAILS-HUMANIZER-START -->

<!-- OB-GUARDRAILS-HUMANIZER-END -->

<!-- OB-GUARDRAILS-LANGFUSE-START -->
## Langfuse (observability skill — MANDATORY LOAD)

- **You MUST call `skill("langfuse")` via the skill tool before any Langfuse work** — instrumenting an application or function, migrating prompts into Langfuse, capturing user feedback as scores on traces, debugging traces, upgrading/migrating Langfuse SDKs, judge calibration, error analysis, or CI/CD experiment gates.
- **Documentation first**: never implement Langfuse code from memory — the product changes frequently. Fetch current docs before writing code: start from `https://langfuse.com/llms.txt`, fetch individual pages as markdown (append `.md` to the path), or search via `https://langfuse.com/api/search-docs?query=<url-encoded>`. Changelog posts confirm a feature exists; never implement from them — use the docs and API/SDK reference.
- **CLI for data access**: use `npx langfuse-cli api ...` (discover resources via `npx langfuse-cli api __schema`, per-resource help via `npx langfuse-cli api <resource> --help`) for querying or modifying Langfuse data. Consult the matching `references/` file under the skill for the use case (instrumentation, prompt-migration, user-feedback, cli, sdk-upgrade, judge-calibration, error-analysis, ci-cd).
- **Credentials**: set `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, and `LANGFUSE_BASE_URL` (or `LANGFUSE_HOST`) as environment variables; never ask for or paste keys into chat, logs, or output. If keys are missing, ask the user to set them in their shell or a `.env` file.
- **Versions**: use the latest Langfuse SDK/API versions unless the user specifies otherwise; be explicit about the exact version to use in any plan handed to another agent.
<!-- OB-GUARDRAILS-LANGFUSE-END -->

<!-- OB-GUARDRAILS-DROID-KNOWLEDGE-BASE-START -->
## DROID knowledge base (reference skill)

- **You MUST call `skill("droid-knowledge-base")` via the skill tool before answering any question that needs the DROID manual** — RAM/memory limits, circuit parameters and defaults, installation, controller specs, MIDI, calibration, `.ini` patch semantics, or "what does the manual say about X". It complements `droid-circuit-reference` (structured schema) with full-text semantic search of the manual itself (`droid-manual-blue-7.md`, 1013 chunks incl. figures). Not project-specific — applies to any DROID question on any machine that can reach `nuc25.local` or the tailnet.
- **Query path**: use the droid **search app** (`bfc81d86a28911f19d893d24d8da86cf`) via the RAGFlow API on `nuc25.local:9380` with a fresh `RAGFLOW_API_KEY` (the env var is stale/401 — generate one in the RAGFlow UI at `https://nuc25-rag.taildec1bd.ts.net` → user-setting → API). Raw `/api/v1/retrieval` and the `ragflow_retrieval` MCP tool return 0 chunks on this KB; the search app is the only working path.
- **Never trust chat assistants for DROID answers** — the droid chat assistant was deleted 2026-08-28 for hallucinating (DROID vs Android); use the search app or the static references instead.
- **Credentials**: API keys live in the environment or a secret store; never paste them into chat, logs, or output.
<!-- OB-GUARDRAILS-DROID-KNOWLEDGE-BASE-END -->

## Engineer workflow (when spawned)

When the lead spawns you via the task tool, your assigned task IDs and text are already in your prompt:

1. Load ALL skills listed under your own `## Abilities` now (Guardrails first, then the rest), by calling the `skill` tool once per `@skill-name`.
2. Gather context using the project-selected tools described above.
3. Implement your assigned tasks in dependency order. Edit only files within your assigned scope.
4. Run the project's tests/lint before marking done (see Code above).
5. Record the task result through the project-selected workflow.
6. Return a summary containing: task IDs done, files changed, tests/lint result, and any decisions made. Then you exit; you do not poll, claim, or wait for more work.
