# ARCHITECTURE.md

## Architecture Overview

`droid_tui` is a single-crate Rust terminal application for loading, inspecting, and interacting with DROID hardware patch files (`.ini`). It renders the hardware components a patch defines — buttons, knobs, CV I/O, encoders, LEDs, switches — grouped into labeled panels that mirror the physical controller layout (P2B8, Faderbank, Notebuttons, …), and supports keyboard and mouse interaction plus shift-group visualization. An embedded source pane, opened with `g` then `v`, shows line-accurate patch text beside the panels and links selected hardware components to their occurrences and modifier relationships.

The system is a **layered monolith** with no framework, no async runtime, and no network: a single-threaded event loop reads terminal events, mutates an in-memory application state, and redraws the screen. The domain model and `.ini` parser are pure functions over strings; the renderer owns all layout decisions and publishes per-frame geometry back to the state for mouse hit-testing.

The app is a **patch viewer/interactor, not a hardware bridge**: it parses `.ini` files and simulates component state locally. It does not connect to DROID hardware.

## 1. Project Structure

```
droid_tui/
├── Cargo.toml              # crate manifest; deps: ratatui, crossterm, color-eyre
├── Cargo.lock
├── src/
│   ├── main.rs             # entry point, terminal lifecycle, event loop
│   ├── app.rs              # App state struct + picker helpers
│   ├── handler.rs          # keyboard + mouse event handling
│   ├── patch.rs            # domain model (Patch, HwComponent, …) + .ini parser
│   └── ui.rs               # ratatui rendering (panels, components, status, picker)
├── fixtures/               # test fixtures: arpeggio1.ini, picker_test/
├── openspec/
│   ├── changes/            # OpenSpec change proposals (archive/ holds completed ones)
│   └── specs/              # capability specs: controller-panels, file-picker,
│                           #   mouse-interaction, patch-parsing, shift-visualization,
│                           #   keybinding, module-scaling, module-orientation,
│                           #   viewer-layout, source-navigation
├── .opencode/              # agent orchestration config (engineers, source roots, platform)
├── .agents/skills/         # project skills (guardrails, ratatui, rust, openspec, …)
├── .beads/                 # beads issue tracker data (Dolt-backed, tooling only)
├── .claude/skills/         # Claude Code skills (droid-patch-format, verify, …)
├── droid_living_examlpes/  # symlink to local DROID reference checkout (machine-local, untracked)
└── target/                 # build artifacts (partially tracked in git — see §15)
```

## 2. High-Level System Diagram

```mermaid
flowchart LR
    USER[User] -->|keys / mouse| TERM[Terminal]
    TERM -->|crossterm events| LOOP[Event Loop<br/>main.rs::run]
    LOOP --> HANDLER[handler.rs<br/>handle_event / handle_mouse_event]
    HANDLER -->|mutates| APP[App state<br/>app.rs]
    HANDLER -.->|g v opens| SOURCE[Embedded source pane]
    APP --> PATCH[Patch model<br/>patch.rs]
    PATCH -->|parses| INI[.ini patch file]
    LOOP -->|draw| UI[ui.rs render]
    UI -->|reads| APP
    UI -->|writes component_rects| APP
    UI -->|renders| TERM
```

## 3. Core Components

### 3.1 User Interface (`src/ui.rs`)

- **Responsibility**: render the entire screen from `App` state each frame; compute layout; publish component geometry for mouse hit-testing.
- **Key functions**: `render` (picker overlay vs. header/main/status split), `render_patch` (groups components into controller panels, wraps to rows, applies shift-group border colors, records `component_rects`), `render_component`, `render_status`, `render_picker`, `render_embedded_main`, `render_source_pane`, `render_source_sidebar`, `render_source_content`, `render_minimap`, and `render_viewer_status`.
- **Technologies**: ratatui 0.29 (`Frame`, `Layout`, `Flex`, `Block`, `Paragraph`), crossterm colors/modifiers.
- **Inputs**: `&mut App`; **Outputs**: terminal frame; side effect: `app.component_rects` filled per frame.
- **Key invariant**: layout is recomputed fresh from `frame.area()` on every draw — terminal resize needs no state handling.

### 3.2 Domain Model & Parser (`src/patch.rs`)

- **Responsibility**: typed model of a DROID patch and a hand-rolled `.ini` parser that builds it.
- **Types**: `Patch` (name, `hw_components`, `modules`, `sections`, raw lines, token spans, occurrence index, modifier index, `shift_groups`), `Span` (0-based line and byte-column range), `ModifierAffect` (resolved modifier span/source/selectat), `HwComponent` (id, label, kind, shift_group, state, controller), `ComponentKind` (Button, CvIn, CvOut, Knob, Switch, Led, Encoder), `ComponentState` (Off, On, Value(f32), Active), `ShiftGroup` (Group1–4 with `color()`/`key_label()`), `Module` / `ModuleWidth`, `IniSection`, and `ViewerCircuit` for prettified blocks.
- **Key functions**: `Patch::from_ini_file` / `from_ini_str` / `sample`, `parse_ini_sections` (comment stripping, repeated-section preservation and header spans), `collect_token_spans`, `scan_hw_tokens` (boundary-aware token scanner), `build_occurrence_index`, `build_modifier_index` (cycle-safe `select`/`selectat` resolution), `token_kind`, `add_component`; rack-recognition API `module_types` / `needs_by_type` / `master_requirement`; `occurrences_for`, `modifier_affected_spans`, `modifier_entries_for`, and `viewer_circuits`.
- **Inputs**: `.ini` file content; **Outputs**: `Result<Patch, String>` (descriptive errors, never panics on malformed input).
- **Design notes**: the parser is deliberately custom (the `ini` crate was removed from `Cargo.toml`) to preserve repeated section names and control token extraction precisely.

### 3.3 Input Handling (`src/handler.rs`)

- **Responsibility**: translate terminal events into `App` mutations.
- **Key functions**: `handle_event` (priority order: picker → armed prefix → embedded-viewer focus → normal keys; keyboard: `q`/Ctrl+C quit, `l` open picker, `g` arms a vim-style prefix (`g v` opens the embedded source pane), `t` toggles raw/prettified mode, Tab switches pane focus, `+`/`-` cycle scale presets 50 %–200 % with wrap-around, `1`–`4` shift groups, `o` toggle portrait/landscape orientation, `Esc` closes the pane or cancels prefix, Enter/Space toggle/select components, `j`/`k` scroll or navigate, and Up/Down/Home/End navigate occurrences), `handle_mouse_event` (hover highlight, panel click toggle/select, empty-space deselection, scroll ±0.05 on knobs/faders, minimap click-to-scroll), `handle_picker_event` (directory navigation, Enter on dir/`.ini`, Esc cancel), and `rect_contains` hit-testing.
- **Inputs**: `KeyEvent`/`MouseEvent`; **Outputs**: `bool` (quit flag) or `()`; mutates `App`.
- **Key invariant**: mouse hit-testing uses `app.component_rects` written by the renderer — the renderer, not the handler, knows where components actually landed on screen.
- **Viewer focus**: `ViewerFocus::Source` isolates panel actions; Tab returns focus to panels, while Esc closes the pane and keeps selection and source position. Picker remains highest priority.

### 3.4 Application State (`src/app.rs`)

- **Responsibility**: single mutable state object threaded through the whole app.
- **Fields**: `patch: Option<Patch>`, `active_shift: Option<ShiftGroup>`, `hovered_component: Option<usize>`, `status_message`, file-picker state (`showing_picker`, `picker_dir`, `selected_file`, `picker_entries`, `picker_index`), `component_rects: Vec<(usize, Rect)>`, `scale_factor: f32` (uniform component-cell scaling applied by the renderer), `orientation: Orientation` (Portrait/Landscape panel direction), `prefix: Option<PrefixState>` (armed vim-style prefix + start instant for the lazy 1 s timeout), and embedded viewer state (`showing_viewer`, `selected_component: Option<String>`, `viewer_focus: ViewerFocus`, `source_view_mode: SourceViewMode`, `occurrence_cursor`, `source_scroll`, `minimap_rect`).
- **Key functions**: `App::new`/`Default`, `load_patch` (stores the patch and resets source-navigation state), `select_component` (selects token and jumps to its first occurrence), `clear_selected_component`, `jump_to_occurrence`, `refresh_picker_entries`, `load_sample_patch`; free function `is_entry_selectable` (`.ini` files and directories selectable, others dimmed).

### 3.5 Entry Point & Event Loop (`src/main.rs`)

- **Responsibility**: process lifecycle and the draw→read→dispatch loop.
- **Key functions**: `main` (color-eyre install, `ratatui::init()`, mouse capture enable, panic-hook chaining to disable mouse capture on panic, `run`, restore), `run` (loop: `terminal.draw(render)` → `event::read()` → dispatch key/mouse/resize).
- **Technologies**: crossterm 0.28 (`EnableMouseCapture`/`DisableMouseCapture`, `event::read`), ratatui `DefaultTerminal`.

## 4. Data Flow

**Main runtime loop** (per frame):

```mermaid
sequenceDiagram
    participant T as Terminal
    participant L as main.rs::run
    participant H as handler.rs
    participant A as App
    participant U as ui.rs::render
    T->>L: event (key/mouse/resize)
    L->>H: dispatch
    H->>A: mutate state (toggle, navigate, shift, picker)
    L->>U: terminal.draw(render)
    U->>A: read state
    U->>U: group components into panels, wrap, style
    U->>A: write component_rects (geometry for hit-testing)
    U->>T: draw frame
```

**Key user journeys**:
- **Load a patch**: press `l` → picker overlay lists current dir → navigate with `j`/`k`/arrows → Enter on `.ini` → `Patch::from_ini_file` parses → picker closes → panels render.
- **Toggle a component**: Enter/Space on hovered component, or mouse click on a component rect → `toggle_component` flips `ComponentState` → status bar shows "Toggled: <label>".
- **Shift visualization**: press `1`–`4` → `active_shift` set → panels containing matching `shift_group` get bold colored borders, others dim; `Esc` clears.
- **Open the source viewer**: press `g` then `v` within 1 s → `open_embedded_viewer` sets `showing_viewer`, focuses the source pane, and starts at BOF or the selected component's first occurrence. Raw lines render by default; `t` switches to prettified circuit blocks. `j`/`k` scroll, Up/Down/Home/End navigate selected-token occurrences, Tab changes focus, and Esc closes while keeping selection and scroll.
- **Scale modules**: press `+`/`-` → cycle presets 50 % → 100 % → 150 % → 200 % (wrapping at both ends) → the renderer multiplies the component cell size; status bar shows "Scaling: N%".
- **Resize**: `Event::Resize` is ignored — the next `draw` recomputes layout from the new `frame.area()`.

## 5. Data Stores

**None.** The application holds all state in memory (`App`) and persists nothing. The `.beads/` directory is the beads issue-tracker's Dolt-backed store, developer tooling, not application data. No database, no schema, no migration strategy.

## 6. External Integrations / APIs

The app reads local `.ini` files and simulates component state; it does not talk to DROID hardware, MIDI, or any network service. The source viewer is rendered in-process from the loaded `Patch`; it uses no subprocess, terminal multiplexer, terminal emulator, IPC, or network integration.

DROID reference material (`droid_living_examlpes/`) remains a machine-local symlink used for development reference only.

## 7. Key Technologies

| Technology | Version | Architectural relevance |
|---|---|---|
| Rust | 2021 edition | Single-crate monolith; no unsafe code |
| ratatui | 0.29 | Terminal UI: `DefaultTerminal`, `Layout`/`Flex`, widgets; owns raw-mode/alternate-screen lifecycle via `init()`/`restore()` |
| crossterm | 0.28 | Event source (`event::read`), mouse capture enable/disable |
| color-eyre | 0.6 | Error reporting + panic hook (chained to also disable mouse capture) |
| serde | 1 | Serialization derives for the in-memory patch domain model; no persistence layer |
| OpenSpec | — | Change proposals + capability specs under `openspec/` |

## 8. Deployment & Infrastructure

- **Build**: `cargo build` (debug) / `cargo build --release`; single native binary `droid_tui`.
- **CI/CD**: none (no `.github/workflows`, no GitLab CI).
- **Containerization**: none.
- **Environment config**: no application-specific environment configuration; the picker starts in `std::env::current_dir()`.
- **Git**: no remote configured; branches `master` (initial commit) and `feature/droid-patch-tui` (active work); archive branch `archive/droid-patch-tui` holds the archived `droid-patch-tui` change plus synced main specs.

## 9. Security Architecture

- **Trust boundary**: the app runs locally with the user's privileges; the only file input is local `.ini` files. The embedded viewer executes no external commands.
- **Input validation**: the parser is defensive — malformed/empty files return descriptive `Err(String)` and never panic (tested: `rejects_empty_file`).
- **Secrets**: none handled, none stored.
- **Auth/authz**: not applicable (no network, no multi-user).
- **Terminal hygiene**: raw mode, alternate screen, and mouse capture are restored on normal exit and on panic (chained panic hook).

## 10. Monitoring & Observability

- **Logging**: none (no logging framework).
- **Error reporting**: color-eyre renders panics/errors to the terminal.
- **Metrics/tracing**: none.
- **Observability gap**: no way to observe runtime behavior outside the TUI itself; acceptable for a local interactive tool.

## 11. Performance & Scalability

- **Model**: single-threaded, no concurrency, no caching.
- **Per-frame cost**: layout and panel grouping are recomputed every draw — O(n) in component count with small constants (`COMPONENT_WIDTH = 16`, `COMPONENT_HEIGHT = 2`). Fine for real DROID patches (tens of components).
- **Known bottleneck**: none at current scale; a pathological patch with thousands of components would redraw slowly, but DROID hardware RAM limits make this unrealistic.

## 12. Development Workflow

- **Setup**: `cargo build` (no install step; no remote to clone from).
- **Test**: `cargo test` (117 unit tests).
- **Lint**: `cargo clippy --all-targets --all-features --locked -- -D warnings`.
- **Format**: `cargo fmt --check` / `cargo fmt`.
- **Verify binary**: `.claude/skills/verify/SKILL.md` drives the built binary interactively.
- **Agent orchestration**: `.opencode/` defines 4 specialized engineers (rusty, layout-designer, horst, dermannmitdermachine), `maxConcurrent: 3`; platform: backlog = browser, repo = none.
- **Change workflow**: OpenSpec — propose under `openspec/changes/`, implement, archive to `openspec/changes/archive/` and sync specs to `openspec/specs/`.

## 13. Testing Strategy

- **Location**: in-module `#[cfg(test)]` unit tests in `patch.rs`, `handler.rs`, and `ui.rs`, plus cross-layer tests in `regression.rs`.
- **Coverage**: 117 tests cover parser spans, raw-line round trips, occurrence indexes, cycle-safe modifier graphs, rack recognition, selection-driven jumps, focus isolation, occurrence navigation, picker and minimap mouse behavior, and UI frames for raw/prettified source, highlights, minimap geometry, narrow layouts, panels, shifts, and status.
- **Frameworks**: std test harness only; no mocking, no property tests, no live-terminal end-to-end test, no coverage gate.
- **Gap**: no end-to-end test driving the real binary; UI tests render into a test `Frame` rather than a live terminal.

## 14. Architectural Decisions & Rationale

1. **Hand-rolled `.ini` parser over the `ini` crate** — preserves repeated section names (DROID patches repeat `[button]` etc.) and gives precise control over token extraction; the crate was removed from `Cargo.toml`.
2. **Boundary-aware hardware-token scanner** — a token starts at a letter (`B/L/P/O/I/E/S`) followed by a digit with a non-alphanumeric/underscore boundary before it, so internal variables like `_ENV1_DECAY_POT` are not misread as hardware tokens.
3. **Components grouped by physical controller** — `HwComponent.controller` ("P2B8", "Faderbank", …) drives panel grouping, mirroring the hardware layout (design.md Decision 3).
4. **Renderer owns layout; handler consumes geometry** — `component_rects` is written by `ui.rs` each frame and read by `handler.rs` for mouse hit-testing, because only the renderer knows where components actually landed.
5. **Fresh layout per frame** — no resize state; `Event::Resize` is a no-op and the next draw reflows automatically.
6. **Chained panic hook** — ratatui's hook restores raw mode/alternate screen; the app chains `DisableMouseCapture` before it so a panic leaves a clean terminal.
7. **Shift groups as an enum** — `ShiftGroup::Group1–4` with `color()`/`key_label()`; panel borders and status bar derive from one source of truth.
8. **Vim-style `g` prefix with lazy timeout** — arming stores only `PrefixState { started: Instant }`; expiry against `PREFIX_TIMEOUT` (1 s) is checked when the next event arrives instead of running a timer thread, keeping the event loop single-threaded and synchronous.
9. **Embedded source viewer** — `g v` opens a source pane in the same TUI and `App`; raw lines and parser-recorded spans support selection jumps, occurrence navigation, modifier highlights, and minimap interaction without IPC or a process boundary.

## 15. Constraints, Risks, and Technical Debt

- **`target/` partially tracked in git** (725+ files) — build artifacts committed at some point; `.gitignore` only covers beads/Dolt files. Hygiene debt; `git add -A` can accidentally sweep build output into commits.
- **No README** — project has no user-facing documentation.
- **ARCHITECTURE.md / DESIGN.md** were placeholders until 2026-08-20; both now hold full generated content and are maintained incrementally.
- **Archived change `droid-patch-tui`** has 48 tasks never checked off in `tasks.md` (process debt; implementation is complete and tested).
- **Picker row styling** (selected yellow bold, non-selectable dim) was computed but never applied to rendered output; removed as dead code during verification. Per-row styling is a design follow-up.
- **No hardware integration** — component state is simulated; wiring to real DROID hardware (e.g., MIDI SysEx upload) is future work.
- **Single-threaded redraw** — fine at current scale; no headless/scriptable mode.
- **Source pane scales with terminal width** — sidebar and minimap hide below their width thresholds to preserve a usable source area; very narrow terminals show only source content.

## 16. Future Considerations

- **Hardware bridge**: upload patches to a running DROID rack via USB-MIDI SysEx (see `droid-hardware-setup` skill) and reflect real state.
- **Schema validation**: validate parsed patches against the authoritative DROID circuit schema (`droid_living_examlpes/droid-lsp/src/circuits.json`, 76 circuits, 10 controllers).
- **Per-row component styling** in the picker (restore the removed style intent).
- **Persistence**: export/import of component state remains a possible future feature; serde derives currently serve the in-memory domain model only.
- **README + DESIGN.md** generation (`/make-design`).
- **CI**: add a workflow running fmt/clippy/test on push.

## 17. Project Identification

- **Name**: droid_tui
- **Language**: Rust (edition 2021)
- **Type**: terminal UI application (ratatui)
- **Runtime**: native binary; Linux
- **Date of review**: 2026-08-23
- **Maintainer**: not evident from the repository

## 18. Glossary / Acronyms

- **DROID**: Der Mann mit der Maschine — Eurorack modular-synthesizer controller hardware; patches are `.ini` files.
- **Hardware token**: address of a physical control in a patch, e.g. `B1.1` (button), `L1.2` (LED), `P1.1` (pot), `O1` (CV out), `I1` (CV in), `E1.1` (encoder), `S1.3` (switch).
- **Controller**: physical panel type — P2B8, Faderbank, Notebuttons, Encoder, Pot, Unusedfaders, etc.
- **Shift group**: a set of components whose behavior/labels change while a shift key (1–4) is held.
- **Source viewer**: embedded readonly source pane showing raw `.ini` lines or prettified circuit blocks, with sidebar, selection-driven highlighting, occurrence navigation, and optional minimap; opened with `g` then `v`.
- **Prefix key**: an armed `g` waits up to 1 s for a follow-up key (e.g. `v`); expiry is checked lazily on the next event.
- **Viewer focus**: `ViewerFocus::Panels` or `ViewerFocus::Source`; controls whether panel or source-pane keys act.
- **ratatui / crossterm**: Rust TUI framework / terminal backend.
- **OpenSpec**: spec-driven change workflow (`openspec/changes/`, `openspec/specs/`).
- **beads (bd)**: Dolt-backed issue tracker used for task tracking.

<!-- Last updated: 2026-08-23T19:12:34+02:00 -->
