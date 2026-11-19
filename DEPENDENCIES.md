# Dependencies

Every crate this repository resolves, why it is here, the features in use, and how much
tree it drags in. The point is subtraction: a crate without a proven consumer is removed,
and a crate used only by tests belongs in `[dev-dependencies]` (YAGNI for crates).

Measured **2026-11-19** with `cargo 1.100.0`, `cargo tree` over both manifests, no
`Cargo.lock` changes. A "tree" figure below is the number of **unique packages** in that
crate's own subtree (normal edges), so the figures overlap and sum past the project total.

```
app manifest                314 packages (239 on normal edges)
tools/screenshots manifest  296 packages (pinned to the app's resolved versions)
```

## Runtime dependencies (`Cargo.toml` → `[dependencies]`)

| crate | version | features in use | tree | why it is here |
|---|---|---|---|---|
| `winit` | 0.30.13 | `default` (x11, wayland, rwh_06, sctk-adwaita, memmap2) | 93 | The only window/event loop. `main.rs` owns the `EventLoop`; `handler.rs` converts key/mouse events; `gui/mod.rs` creates the window. |
| `egui` | 0.36.1 | `default` (default_fonts), `bytemuck` | 66 | Every surface's paint code (`gui/*`), the shared `egui::Shape` output the tests assert on, and the `theme::egui_color` bridge. |
| `egui-winit` | 0.36.1 | `default` (clipboard/arboard, wayland, x11, links) | 205 | Bridges winit input and IME/clipboard state into egui — the largest subtree here, because it pulls the whole Wayland/X11 stack. |
| `egui-wgpu` | 0.36.1 | `default`, `fragile-send-sync-non-atomic-wasm` | 115 | egui's wgpu painter: turns the frame's shapes into GPU draw calls. |
| `wgpu` | 30.0.1 | `default` (vulkan, gles, metal, dx12, web*), `parking_lot`, `wgsl` | 81 | The GPU device/surface/queue behind `egui-wgpu`; `gui/mod.rs` owns the surface configuration. |
| `serde` | 1.0.229 | `derive`, `std` | 9 | Every persisted or reported shape: `config`, `app`, `patch`, `physical`, `geometry`, `schema`, `report`, `plugin`, `favorites`, `store`. |
| `serde_json` | 1.0.151 | `std` | 5 | The CLI's JSON report (`report`/`cli`), and parsing the two embedded JSON data files (`circuits.json` in `schema`, the geometry files in `geometry`/`physical`). |
| `toml` | 0.9.12 | `default` (parse, display, serde, std) | 8 | The user stores (`config.toml`, `labels.toml`, `favourites.toml`, `plugins/*.toml`) read **and** written through `config.rs`, `store.rs`, `plugin.rs`. |
| `color-eyre` | 0.6.5 | `default` (track-caller, tracing-error, capture-spantrace) | 29 | `main.rs`'s error result and the human-readable failure report on a bad startup; not used in the library. |
| `egui_kittest` | 0.36.1 | `snapshot`, `wgpu` | 168 | **Test-only.** Runs a real egui frame headlessly and exposes the painted widgets through AccessKit, so the graph journeys assert `get_by_label("<circuit>")` after an input-driven repaint. Belongs in `[dev-dependencies]` — see `droid_tui-4fy`. |

## Dev dependencies (`Cargo.toml` → `[dev-dependencies]`)

| crate | version | features | tree | why it is here |
|---|---|---|---|---|
| `tempfile` | 3.27.0 | `default` | 9 | `TempDir`/`tempdir` for every test that writes a config, store, plugin or staged `.syx` file without touching the real data directory. |

## Tools crate (`tools/screenshots/Cargo.toml`)

A separate crate that path-shadows `src/` to render `docs/assets/*.png`. It repeats the
window/GPU stack because it compiles the app's real paint path, pinned with `=` to the
versions `Cargo.lock` resolves so the harness and the tests build one tree.

| crate | why |
|---|---|
| `egui`, `egui-wgpu`, `egui-winit`, `winit`, `wgpu`, `egui_kittest` | the app's paint stack, exactly as above, plus the harness for the shots |
| `image` | `default-features = false`, features `png` only — snapshots. The default set pulls `ravif`/`rav1e`, which no longer resolves offline. |
| `serde`, `serde_json`, `toml` | the shadowed `src/` modules it stages need them to compile |

## Transitive facts worth knowing

- **239 packages on normal edges**, 75 more from build/dev edges (proc macros, `cc`, the
  test stack). Nothing here is a second GUI or a second parser.
- **15 crate names resolve to two versions.** The clusters: Wayland/X11 (`calloop` 0.13/0.14,
  `smithay-client-toolkit` 0.19/0.20, `calloop-wayland-source`, `rustix`, `linux-raw-sys`),
  proc-macro drift (`syn` 2/3, `thiserror` 1/2, `hashbrown`, `rustc-hash`), compression
  (`miniz_oxide`), and **`toml` 0.9 + 1.1** — our direct pin is 0.9 while something in the
  tree asks for 1.x. Moving the direct dependency to 1.x is the one duplicate this
  repository can collapse on its own; the rest belong to `winit`/`wgpu`.
- **`egui-winit` is the widest leaf to prune.** It alone carries 205 packages because of the
  Wayland/X11 client stack. If the app ever settles on one display backend, that is where
  the tree shrinks.

## Audit playbook

Run this whenever a dependency question comes up, and at least once per release:

```bash
# 1. want-level: is the crate used at all, per target?
cargo machete Cargo.toml tools/screenshots/Cargo.toml
cargo +nightly udeps --all-targets          # only if a nightly is already present

# 2. feature-level: which features are actually enabled, and are they all needed?
cargo tree -e normal --depth 1 --format "{p} | {f}"

# 3. transitive: what comes along, and what resolves twice?
cargo tree -e normal --prefix none --format "{p}" | sort -u | wc -l
cargo tree -p <crate> --prefix none --format "{p}" | sort -u | wc -l

# 4. prove the consumer before removing anything
rg -n "<crate>::" src/ tests/ tools/
```

Verdicts, in order of preference: **remove** (no consumer), **move to
`[dev-dependencies]`** (consumed only under `#[cfg(test)]`), **narrow the features**
(consumer exists, half the features do not), **keep with a reason** (recorded in the table
above). Removals are subtraction only — never `cargo update`, never a version bump in the
same change, and `Cargo.lock` may only shrink by the removed crates.

<!-- Last updated: 2026-11-19 · initial dependency inventory: 10 runtime, 1 dev, 8 tool-crate deps; 314 packages (239 normal edges); 15 doubly-versioned crate names, of which `toml` 0.9 vs 1.1 is the only one this repo can collapse itself; `egui_kittest` recorded as test-only pending droid_tui-4fy. -->
