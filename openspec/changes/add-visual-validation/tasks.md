# Tasks: add-visual-validation

## 1. Snapshot harness and start-small coverage

- [x] 1.1 Add `insta` dev-dependency and snapshot helper — add `insta = "1"` to `[dev-dependencies]` in `Cargo.toml`, update `Cargo.lock` via `cargo build --locked`, create `buffer_to_ansi` + `buffer_to_html` helpers in `src/regression.rs` (ANSI trims trailing empty cells, HTML maps fg/bg/bold/dim/reversed per span), `.gitignore` `snapshots/` and `evidence/gallery/`, and verify `cargo test -- --list` still green <!-- agent: rusty-engineer.build, depends_on: [], touches: [Cargo.toml, Cargo.lock, src/regression.rs, .gitignore] -->
- [x] 1.2 Controller-panels snapshots — `arpeggio1.ini` × `classic`/`terminal`/`mono` × widths 80/120, assert P2B8 8 buttons + 2 knobs face + style tokens (kind colors, muted chrome), via `insta` inline snapshot of ANSI; verify `cargo insta test --check` shows snapshots as pending <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/regression.rs, src/ui.rs] -->
- [x] 1.3 Boxed vs plain + viewer-layout snapshots — `led_pairs.ini` mixed boxed/text grid and `source_navigation.ini` viewer open/closed at width 100, assert boxed border kind-colored and folded LED not standalone (per droid_tui-5mj/droid_tui-1hg); verify insta snapshots pending <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/regression.rs, src/ui.rs, fixtures/led_pairs.ini] -->
- [x] 1.4 Theming + shift snapshots — same fixtures with `shift1` active (bold colored border + `SHIFT 1 ACTIVE` chip) and `mono` grayscale pairwise distinct, side-by-side html row; verify `cargo test` fails until insta accept <!-- agent: layout-designer-engineer.build, depends_on: [1.1], touches: [src/regression.rs, src/theme.rs, src/ui.rs] -->

## 2. Gallery and strict gate

- [x] 2.1 HTML gallery renderer — generate `evidence/gallery/index.html` with one row per scenario (columns classic/terminal/mono, widths 80/120, viewer open/closed, shift active), each cell being the HTML from 1.1; add `cargo test -- --generate-gallery` / `cargo run --bin snapshot-gallery` entrypoint; verify gallery opens in browser and matches ANSI content <!-- agent: layout-designer-engineer.build, depends_on: [1.2, 1.3, 1.4], touches: [src/regression.rs, tools/snapshot-gallery/**, evidence/gallery/**] -->
- [x] 2.2 Strict `cargo test` auto gate + ephemeral wiring — `cargo test` generates and asserts insta snapshots (no separate make step), `.gitignore` covers pending `.snap` files, CI runs `cargo insta test --check` and fails on diff; verify `cargo test` exits non-zero on intentional face change <!-- agent: rusty-engineer.build, depends_on: [1.2, 1.3, 1.4], touches: [src/regression.rs, .gitignore, Cargo.toml] -->

## 3. Archive and CI durability

- [ ] 3.1 Archive evidence hook — on `openspec archive add-visual-validation`, copy `evidence/gallery/` (HTML + ANSI) into `openspec/changes/archive/add-visual-validation/evidence/gallery/`; add a short archive script/note; verify `openspec archive --help` flow and file presence post-archive <!-- agent: rusty-engineer.build, depends_on: [2.1], touches: [openspec/changes/add-visual-validation/evidence/**, scripts/archive-gallery.sh] -->
- [ ] 3.2 CI ephemeral artifact upload — configure CI to upload `evidence/gallery/` and pending `snapshots/` as artifact (no commit), retain for PR review; verify artifact appears in CI run log <!-- agent: rusty-engineer.fast, depends_on: [2.1], touches: [.github/workflows/**, .gitlab-ci.yml] -->

## 4. Docs and verification

- [ ] 4.1 Regenerate DESIGN.md + ARCHITECTURE.md + guardrails and run full gates — run `/make-design` and `/make-architecture` and `/make-guardrails`, then `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, `cargo test`, `cargo build --release --locked` all green; verify `DESIGN.md` provenance note for visual-validation and `ARCHITECTURE.md` testing section updated <!-- agent: layout-designer-engineer.fast, depends_on: [2.2, 3.1], touches: [DESIGN.md, ARCHITECTURE.md, .agents/skills/ob-guardrails-project/SKILL.md] -->
