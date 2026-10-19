# Tasks: validator-essential-alternatives

Wave plan: `[1]` -> `[2]` -> `[3,4]` (2 and 3 both touch `src/validation.rs`, so they run in one lane, sequentially; 4 is a separate file).

Note on agent tiers: `.opencode/harness.json` pins `models.plan/build/fast` to
`opencode/deepseek-v4.1-flash` while this session runs on provider `opencode-go`,
so the `.build` / `.fast` tier workers are not spawnable right now. Tasks below
therefore name the base specialists (`rusty-engineer`, `horst-engineer`), which
carry the same abilities.

- [x] 1 Add the documented-alternatives table and the `seen`-based satisfaction check to the `essential == 2` arm <!-- agent: rusty-engineer, depends_on: [], touches: [src/validation.rs] -->
- [ ] 2 Add per-row unit tests (positive, negative, and the "alternative present but a different required param absent" case) plus table-sanity checks <!-- agent: horst-engineer, depends_on: [1], touches: [src/validation.rs] -->
- [ ] 3 Add `fixtures/validation/essential_alternatives.ini` — one section per documented alternative (`lfo` with `taptempo`, `gatetool` with `inputtrigger`, `contour` with `trigger`, `compare` with `ifless`) plus one control section that sets none of them — and a validation test asserting zero `missing_required` for the first four and exactly one for the control. Do not reference `patches_remote/_droid_mpfs5drum.ini`: it is a symlink into a working tree outside the repo, so the 14 → 0 measurement stays proposal evidence, not CI material. <!-- agent: horst-engineer, depends_on: [1], touches: [fixtures/validation/**, src/validation.rs] -->
- [x] 4 Update ARCHITECTURE.md §3.10 to state the exception and record the upstream (`circuits.json` / `diagnostics.ts`) divergence for a follow-up report <!-- agent: rusty-engineer, depends_on: [1], touches: [ARCHITECTURE.md] -->

**Total: 4 tasks**
**Agent assignments**: rusty-engineer (1,4), horst-engineer (2,3)
