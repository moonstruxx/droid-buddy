# Change: patch-validation

## Why
`droid_tui` parses patches via `patch.rs::from_ini_str -> Patch` but returns only `Result<Patch, String>` (first error, single `status_message` at `handler.rs:1298`). Users get no line-col list. `droid-lsp/src/diagnostics.ts:provideDiagnostics` already implements 9 line-accurate checks on the authoritative `circuits.json` (76 circuits, 10 controllers, `prefix/count/start_at` expansion + `JACK_TABLE`). Port that schema + rules to Rust so `load_patch` can return `Vec<ValidationIssue{span, severity, code, message}>` with `L10:5 [E] [motor] unknown circuit …`.

Decisions: `1` modal error list · `2` trigger on `load_patch` discovery · `3` fail load on Severity::Error (Wiring-only today is aspirational) · `4` all 9 LSP checks · `5` vendor `droid-lsp` as `ext/droid-lsp` submodule (`ext/droid-lsp/src/circuits.json` single source of truth, `include_str!`, not a copy).

## What Changes
- `ext/droid-lsp` submodule + `src/schema.rs` embeds `circuits.json` (serde mirror of `CircuitDef`, `JACK_TABLE`, `getParamNames` expansion) — replaces stale `KNOWN_CONTROLLER_SECTIONS` (7).
- `src/patch.rs` `IniSection.entries` extended from `Vec<(String,String)>` to per-entry `EntrySpan { key, value, key_span: Span, value_span: Span, line }` — retains `header_span` + adds value-level ranges (reuse `patch.rs:1188` `scan_hw_tokens_with_spans` pattern).
- New pure `src/validation.rs` `Severity{Error,Warning,Hint}` + `ValidationIssue{span, severity, code, message}` + `fn validate_patch(&Patch,&Schema)->Vec<ValidationIssue>` — 9 checks ported 1:1 (unknown circuit → Error + Levenshtein, duplicate param → Warning, unknown param → Error, invalid jack → Warning, missing required `essential==2` → Warning at header, undefined `_cable` → Warning, duplicate cable def → Warning, unused cable → Hint, RAM budget → Error), deterministic sort by `(line,col)`.
- `src/app.rs` `validation_issues: Vec<ValidationIssue>`, `showing_validation: bool`, `validation_cursor`; `load_patch` runs `validate_patch`; if any `Error` → `self.patch=None`, `showing_validation=true`, status `Load failed: N errors`; warnings/hints still load but also pop modal.
- `src/ui.rs` `render_validation_modal` + `src/theme.rs` tokens `validation_error/warning/hint` + `src/handler.rs` `e` toggle / `j/k` / `Enter`→`source_scroll` jump / `Esc` dismiss.

## Impact
- Affected specs: new capability `patch-validation`
- Affected code: `src/schema.rs` (new), `src/validation.rs` (new), `src/patch.rs`, `src/app.rs`, `src/events.rs`, `src/ui.rs`, `src/theme.rs`, `src/handler.rs`, `Cargo.toml`, `ext/droid-lsp` (submodule)
- Non-goals: no live-on-edit revalidation, no write-back, no cv/gate value-type checks beyond jack/_cable

## Non-goals
- No live-on-edit revalidation (on-load only)
- No patch write-back / auto-fix
- No `cv`/`gate` value-type validation beyond jack presence
