# Tasks: config-store

## 1. Theme engine (src/theme.rs)

- [x] 1.1 Create `src/theme.rs` <!-- agent: rusty-engineer.build | depends_on: | touches: src/theme.rs --> with the `Theme` token struct (~14 semantic fields per design Decision 1) and the `classic` constructor reproducing today's exact color mapping; unit test asserts `classic` field values equal the current literals from `ui.rs`/`patch.rs`. Verify: `cargo test theme`
- [x] 1.2 Add `terminal` and `mono` constructors <!-- agent: rusty-engineer.build | depends_on: 1.1 | touches: src/theme.rs --> plus canonical name resolution (`THEMES` catalog + case/separator-insensitive lookup with fallback); tests cover normalization aliases, unknown-name fallback, and pairwise-distinct shift tokens for every shipped theme. Verify: `cargo test theme`
- [x] 1.3 Wire `mod theme;` into `main.rs` <!-- agent: rusty-engineer.build | depends_on: 1.2 | touches: src/main.rs --> and expose the session's active theme; verify build stays green (`cargo clippy --all-targets --all-features --locked -- -D warnings`)

## 2. Config store (src/config.rs)

- [x] 2.1 Add `toml` dependency <!-- agent: rusty-engineer.build | depends_on: | touches: Cargo.toml, Cargo.lock, src/config.rs --> to `Cargo.toml`; create `src/config.rs` with the v1 settings struct (`theme` key) and XDG-aware path discovery; test: missing dir/file returns defaults silently, `$XDG_CONFIG_HOME` override honored (tempdir fixture). Verify: `cargo test config`
- [x] 2.2 Implement loading <!-- agent: rusty-engineer.build | depends_on: 2.1 | touches: src/config.rs --> with malformed-file and unknown-theme-name fallbacks (warn-once on stderr); tests use broken TOML and unknown name fixtures asserting defaults apply. Verify: `cargo test config`
- [x] 2.3 Implement atomic write path <!-- agent: rusty-engineer.build | depends_on: 2.2 | touches: src/config.rs --> (temp file + rename, mkdir on demand); test writes into a tempdir and re-reads the result round-trip. Verify: `cargo test config`
- [x] 2.4 Call config load at the top of `main()` <!-- agent: rusty-engineer.build | depends_on: 1.3, 2.3 | touches: src/main.rs --> before `ratatui::init()`, feeding the selected theme; manual smoke check via the `verify` skill: absent config → normal startup, `theme = "mono"` → visibly restyled. Verify: binary run

## 3. Re-point rendering to tokens

- [x] 3.1 Replace all hardcoded colors in `ui.rs` <!-- agent: layout-designer-engineer.build | depends_on: 1.3 | touches: src/ui.rs --> with active-theme token reads (TDD: update affected ui tests to assert via tokens first where they currently pin literals); verify `cargo test ui` and that classic output is unchanged in ui frame tests
- [x] 3.2 Move `ShiftGroup::color()` lookups <!-- agent: rusty-engineer.build | depends_on: 3.1 | touches: src/patch.rs, src/ui.rs --> into the theme token layer keyed by group index; keep `key_label()` on the enum; update patch/ui tests accordingly. Verify: `cargo test` full suite
- [x] 3.3 Regression pass <!-- agent: horst-engineer.build | depends_on: 2.4, 3.2 | touches: src/regression.rs -->: boxed-cell borders, shift borders/status, picker, viewer sidebar/content/status render correctly under `classic`, `terminal`, and `mono` (render into a test Frame per theme). Verify: `cargo test regression`

## 4. Verification & docs

- [x] 4.1 Full gate set green: `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings --locked`, `cargo test`, `cargo build --release --locked`. Verify: all four exit 0
- [x] 4.2 Interactive verification with the `verify` skill: default start identical to before; switching config to each theme changes rendering as specified. Verify: observed behavior recorded

- [x] 3.4 Extend Theme with viewer-highlight tokens (occurrence/current/modifier/focus/minimap-signal per DESIGN.md viewer section) and re-point the 15 remaining literal sites in ui.rs raw/prettified highlights, minimap signals, focused viewer borders <!-- agent: rusty-engineer.build | depends_on: 3.2 | touches: src/theme.rs, src/ui.rs -->
