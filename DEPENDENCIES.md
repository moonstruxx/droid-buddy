# Dependencies

Every dependency this repository has, split by **what it is for** — because that decides
where it belongs and whether it ships:

- a **library dependency** is linked into the shipped binary;
- a **tool dependency** is compiled into a test binary, a build-time harness, or is an
  external command a developer runs. None of it ships.

A dependency that does not ship belongs in `[dev-dependencies]`, in the `tools/` crate, or
in the toolchain — never in `[dependencies]`. That is the same YAGNI rule as a crate with
no consumer: a shipped crate without a runtime consumer is a defect.

Measured **2026-11-19** with `cargo 1.100.0`, `cargo tree` over both manifests. The `risk`
column is the crate's class from [`SECURITY.md`](SECURITY.md); a crate that carries another
crate's finding reports that class. "Tree" is the number of unique packages in that crate's
own subtree (normal edges), so the figures overlap and sum past the project total.

```
shipped binary          314 packages (239 on normal edges)
tools/screenshots       296 packages (pinned to the same versions)
```

## 1. Library dependencies — shipped (`Cargo.toml` → `[dependencies]`)

| crate | version | kind | features in use | tree | risk | why it is here |
|---|---|---|---|---|---|---|
| `winit` | 0.30.13 | library | `default` (x11, wayland, rwh_06, sctk-adwaita, memmap2) | 93 | low | The only window/event loop. `main.rs` owns the `EventLoop`; `handler.rs` converts key/mouse events; `gui/mod.rs` creates the window. |
| `egui` | 0.36.1 | library | `default` (default_fonts), `bytemuck` | 66 | none | Every surface's paint code (`gui/*`), the shared `egui::Shape` output the tests assert on, and the `theme::egui_color` bridge. |
| `egui-winit` | 0.36.1 | library | `default` (clipboard/arboard, wayland, x11, links) | 205 | none | Bridges winit input and IME/clipboard state into egui — the largest subtree here, because it pulls the whole Wayland/X11 stack. |
| `egui-wgpu` | 0.36.1 | library | `default`, `fragile-send-sync-non-atomic-wasm` | 115 | none | egui's wgpu painter: turns the frame's shapes into GPU draw calls. |
| `wgpu` | 30.0.1 | library | `default` (vulkan, gles, metal, dx12, web*), `parking_lot`, `wgsl` | 81 | none | The GPU device/surface/queue behind `egui-wgpu`; `gui/mod.rs` owns the surface configuration and the crate's one `unsafe` site. |
| `serde` | 1.0.229 | library | `derive`, `std` | 9 | none | Every persisted or reported shape: `config`, `app`, `patch`, `physical`, `geometry`, `schema`, `report`, `plugin`, `favorites`, `store`. |
| `serde_json` | 1.0.151 | library | `std` | 5 | none | The CLI's JSON report (`report`/`cli`), and parsing the two embedded JSON data files (`circuits.json` in `schema`, the geometry files in `geometry`/`physical`). |
| `toml` | 0.9.12 | library | `default` (parse, display, serde, std) | 8 | none | The user stores (`config.toml`, `labels.toml`, `favourites.toml`, `plugins/*.toml`) read **and** written through `config.rs`, `store.rs`, `plugin.rs`. |
| `color-eyre` | 0.6.5 | library | `default` (track-caller, tracing-error, capture-spantrace) | 29 | none | `main.rs`'s error result and the human-readable failure report on a bad startup; not used in the library. |

`egui_kittest` is **not** in this table on purpose: it is a tool dependency that currently
sits in `[dependencies]` by mistake (every build warns `unused dependency`). Moving it is
`droid_tui-4fy`.

## 2. Tool dependencies — never shipped

### 2a. Test-only crates (`[dev-dependencies]`)

| crate | version | kind | features | tree | risk | why it is here |
|---|---|---|---|---|---|---|
| `tempfile` | 3.27.0 | tool (test) | `default` | 9 | none | `TempDir`/`tempdir` for every test that writes a config, store, plugin or staged `.syx` file without touching the real data directory. |
| `egui_kittest` | 0.36.1 | tool (test) | `snapshot`, `wgpu` | 168 | none | Runs a real egui frame headlessly and exposes the painted widgets through AccessKit, so the graph journeys assert `get_by_label("<circuit>")` after an input-driven repaint. Pending move: `droid_tui-4fy`. |

### 2b. Build-time harness (`tools/screenshots/Cargo.toml`)

A separate crate that path-shadows `src/` to render `docs/assets/*.png`. It repeats the
window/GPU stack because it compiles the app's real paint path, pinned with `=` to the
versions `Cargo.lock` resolves so the harness and the tests build one tree.

| crate | kind | why |
|---|---|---|
| `egui`, `egui-wgpu`, `egui-winit`, `winit`, `wgpu`, `egui_kittest` | tool (build) | the app's paint stack, exactly as above, plus the harness for the shots |
| `image` | tool (build) | `default-features = false`, features `png` only — snapshots. The default set pulls `ravif`/`rav1e`, which no longer resolves offline. |
| `serde`, `serde_json`, `toml` | tool (build) | the shadowed `src/` modules it stages need them to compile |

### 2c. External commands and services — no crate, no lockfile

These are what the guardrails check *with*. They are all **opt-in**: nothing in the build,
the test suite or CI depends on them, so their absence is never a failure.

| tool / service | kind | installed here? | used for |
|---|---|---|---|
| `cargo`, `rustc` (stable 1.100.0), `rustup` | required toolchain | yes | every gate |
| `python3` (stdlib only) | required tooling | yes | `scripts/codehealth.py`, `tools/acquire_geometry.py`, `tools/build_features.py`, `scripts/security-sync.py` |
| `cargo fmt`, `cargo clippy`, `cargo doc` | toolchain components | yes | the gate set |
| `cargo-miri` (+ nightly) | optional diagnostic | no | Rust-level UB — see `SECURITY.md`/guardrails ladder |
| AddressSanitizer (`RUSTFLAGS="-Zsanitizer=address"`, nightly) | optional diagnostic | no | address errors and leaks in the real binary |
| `cargo-fuzz` (libFuzzer) | optional diagnostic | no | panics/hangs on `Patch::from_ini_str` |
| `cargo-geiger` | optional diagnostic | no | unsafe inventory of the dependency tree |
| `cargo-deny` | optional, no policy yet | no | licence/advisory/bans gate once a policy exists |
| `cargo-machete` | optional diagnostic | no | want-level unused-crate scan |
| `valgrind` | fallback diagnostic | no | memcheck/massif on the headless CLI only |
| OSV (`api.osv.dev`), cve.org (`cveawg.mitre.org`) | network service | reachable | `scripts/security-sync.py` — optional, offline-tolerant |
| OpenCVE (`app.opencve.io`) | network service | 401 without key | second CVE view, only with `OPENCVE_API_KEY` |

## 3. Transitive facts worth knowing

- **239 packages on normal edges**, 75 more from build/dev edges (proc macros, `cc`, the
  test stack). Nothing here is a second GUI or a second parser.
- **15 crate names resolve to two versions.** The clusters: Wayland/X11 (`calloop` 0.13/0.14,
  `smithay-client-toolkit` 0.19/0.20, `calloop-wayland-source`, `rustix`, `linux-raw-sys`),
  proc-macro drift (`syn` 2/3, `thiserror` 1/2, `hashbrown`, `rustc-hash`), compression
  (`miniz_oxide`), and **`toml` 0.9 + 1.1** — our direct pin is 0.9 while something in the
  tree asks for 1.x. Moving the direct dependency to 1.x is the one duplicate this
  repository can collapse on its own; the rest belong to `winit`/`wgpu`.
- **One advisory touched this tree.** `RUSTSEC-2026-0486` (`rustix` xattr UB) was in the
  shipped lock via `egui-winit`'s clipboard path and is remediated at `1.1.6`; the one
  open finding is the unmaintained `ttf-parser`, inherited from `winit`. Full evaluation,
  classification scale and re-run recipe: [`SECURITY.md`](SECURITY.md).
- **`egui-winit` is the widest leaf to prune.** It alone carries 205 packages because of the
  Wayland/X11 client stack. If the app ever settles on one display backend, that is where
  the tree shrinks.

## 4. Audit playbook

Run this whenever a dependency question comes up, and at least once per release:

```bash
# 1. want-level: is the crate used at all, per target?
cargo machete Cargo.toml tools/screenshots/Cargo.toml
cargo +nightly udeps --all-targets          # only if a nightly is already present

# 2. is it shipped? a crate with no runtime consumer belongs in [dev-dependencies]
#    or in tools/, never in [dependencies]
rg -n "use (crate_name)" src/ | grep -v "cfg(test)"

# 3. feature-level: which features are actually enabled, and are they all needed?
cargo tree -e normal --depth 1 --format "{p} | {f}"

# 4. transitive: what comes along, and what resolves twice?
cargo tree -e normal --prefix none --format "{p}" | sort -u | wc -l
cargo tree -p <crate> --prefix none --format "{p}" | sort -u | wc -l

# 5. vulnerability + maintenance scan: every resolved package against OSV, then the
#    CVE record for each advisory - optional, needs network, never a build gate
python3 scripts/security-sync.py
#    (sources, flags and the classification scale: SECURITY.md)
```

Verdicts, in order of preference: **remove** (no consumer), **re-classify as a tool
dependency** (consumed only under `#[cfg(test)]`, by `tools/`, or by a developer command —
`[dev-dependencies]` or the tools crate, never `[dependencies]`), **narrow the features**
(consumer exists, half the features do not), **keep with a reason** (recorded in the table
above). Removals and re-classifications are subtraction only — never `cargo update`, never
a version bump in the same change, and `Cargo.lock` may only shrink by the removed crates.

<!-- Last updated: 2026-11-19 · split by kind: library dependencies (shipped) vs tool dependencies (test-only crates, the tools/screenshots harness, and the external commands/services the guardrails use, all opt-in). `egui_kittest` is listed as a tool dependency pending its move out of [dependencies] (droid_tui-4fy); the audit playbook gained the "is it shipped?" step and the re-classify verdict. Security pass carried over: `risk` column from SECURITY.md, `winit` low via the inherited unmaintained `ttf-parser`, everything else none after the rustix 1.1.6 bump; 314 packages (239 normal edges). -->
