# Tasks: validator-essential-alternatives

Wave plan: `[1]` -> `[2,3,4]` (all three touch disjoint files and depend only on 1).

Note on agent tiers: `.opencode/harness.json` pins `models.plan/build/fast` to
`opencode/deepseek-v4.1-flash` while this session runs on provider `opencode-go`,
so the `.build` / `.fast` tier workers are not spawnable right now. Tasks below
therefore name the base specialists (`rusty-engineer`, `horst-engineer`), which
carry the same abilities.

- [ ] 1 Add the documented-alternatives table and the `seen`-based satisfaction check to the `essential == 2` arm <!-- agent: rusty-engineer, depends_on: [], touches: [src/validation.rs] -->
- [ ] 2 Add per-row unit tests (positive, negative, and the "alternative present but a different required param absent" case) plus table-sanity checks <!-- agent: horst-engineer, depends_on: [1], touches: [src/validation.rs] -->
- [ ] 3 Pin the drum patch family: `patches_remote/_droid_mpfs5drum.ini` yields 0 findings after the change (was 14 `missing_required`) <!-- agent: horst-engineer, depends_on: [1], touches: [src/regression.rs, src/validation.rs] -->
- [ ] 4 Update ARCHITECTURE.md §3.10 to state the exception and record the upstream (`circuits.json` / `diagnostics.ts`) divergence for a follow-up report <!-- agent: rusty-engineer, depends_on: [1], touches: [ARCHITECTURE.md] -->

**Total: 4 tasks**
**Agent assignments**: rusty-engineer (1,4), horst-engineer (2,3)
