# Tasks

## 1. Content + harness

- [ ] 1.1 Deterministic screenshot harness (headless paint renders at fixed fixtures — small specs AND the original large droid mpfs patch — for README/docs) and verify the renders are stable across runs <!-- agent: rusty-engineer.build, depends_on: [], touches: [tools/**] -->
- [ ] 1.2 README.md (motivation, loud disclaimer, screenshots, current/planned features, pain-points query → GitHub Issues, developer onboarding, acknowledgements) and verify links Shots resolve <!-- agent: devops-engineer.fast, depends_on: [1.1], touches: [README.md] -->
- [ ] 1.3 docs/ manual (getting-started, views, keys, labels, uploading, config, contributing) and verify internal links resolve <!-- agent: devops-engineer.fast, depends_on: [], touches: [docs/**] -->

## 2. Verification

- [ ] 2.1 Verify (dead-link check over README + docs, screenshots fresh, full gate green) <!-- agent: horst-engineer.fast, depends_on: [1.2, 1.3], touches: [] -->
