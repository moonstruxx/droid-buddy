# Security

Dependency risk evaluation for this repository. The application has no network, no auth,
no secrets and no multi-user state (§9 of `ARCHITECTURE.md`); the attack surface is the
`.ini` files it parses and whatever the crate graph brings in. This document is about the
second part.

**Scan method, measured 2026-11-19:** every resolved package of both manifests (246 for the
app, 232 for `tools/screenshots`) queried against the [OSV](https://osv.dev) batch API,
which mirrors the RustSec advisory database. `cargo-audit` is not installed in this
environment, so the query was made directly against the same data `cargo audit` reads. A
re-run is one command, below.

```
246 packages queried · 1 memory-safety advisory found and remediated · 1 unmaintained crate accepted
```

## Classification

Severity of the **advisory** first, then a downgrade only for a documented reason (the
vulnerable code path is not reachable from this application). A crate with no advisory is
`none`; a crate that only carries another crate's finding reports that crate's class.

| class | meaning |
|---|---|
| `critical` | remote code execution or memory corruption reachable through normal app use, or an advisory with a known exploit |
| `high` | memory-safety or sandbox-escape defect in code this app can reach, or an unpatched vulnerability in a runtime dependency |
| `medium` | memory-safety defect present in the tree but not reachable from this app's code paths, or a patch level that is known to be fixed upstream |
| `low` | maintenance withdrawal (unmaintained crate, no known defect), or a finding confined to test/dev-only code |
| `none` | no advisory affects the resolved version |

## Findings

### RUSTSEC-2026-0486 · GHSA-jh2m-fhpj-3vr7 — `rustix` — **remediated**

- **Affected:** `rustix` 1.0.0 – 1.1.5. The `xattr` functions in `rustix::fs`
  (`getxattr`, `lgetxattr`, `fgetxattr`, `listxattr`, `llistxattr`, `flistxattr`) mark
  bytes as initialised that the kernel never wrote when the buffer has zero length, which
  is undefined behaviour reachable from safe code.
- **Reached how:** `egui-winit` → `smithay-clipboard` → `smithay-client-toolkit` 0.20 →
  `calloop` 0.14 → `rustix` 1.1.4. Runtime dependency, Linux/Wayland clipboard path.
- **Reachable here?** No. Nothing in this repository calls an extended-attribute
  function, and none of the crates on that path uses `spare_capacity` with a zero-length
  buffer. The defect was nevertheless latent in the shipped tree.
- **Fixed:** `rustix` 1.1.6. Resolved by a targeted patch bump (`Cargo.lock` and
  `tools/screenshots/Cargo.lock`, `1.1.4 → 1.1.6`, nothing else moved); a re-query of
  1.1.6 returns no advisory. Class before the fix: **medium**; after: **none**.
- **Note:** `rustix` 0.38.44 also resolves in the tree (through `winit`'s older
  `sctk-adwaita` path) and is outside the advisory's affected range.

### RUSTSEC-2026-0192 — `ttf-parser` — **accepted, low**

- **Status:** unmaintained, not vulnerable. The author has stated the crate will receive
  no further fixes; the advisory recommends `skrifa` (Google Fonts `fontations`).
- **Reached how:** `winit` → `sctk-adwaita` → `ab_glyph` → `owned_ttf_parser` →
  `ttf-parser` 0.25.1. It parses the Wayland window-decoration font, i.e. a font file that
  ships with the system theme, not input from the user or the network.
- **Why accepted:** there is no known defect to act on, the parsed input is not
  attacker-controlled, and the crate is pinned by `winit`. Replacing it means replacing
  `sctk-adwaita` (and thereby `winit`'s decorations) or vendoring a fork — both larger than
  the risk. Class: **low**.
- **Watch:** it disappears when `winit`/`sctk-adwaita` move to `skrifa` or drop
  `ab_glyph`. Re-check on every `winit` upgrade.

## Sources and the optional web sync

The scan reads three services. Two answer without credentials, which is why they carry the
evaluation; the third is opt-in:

| service | what it contributes | auth | used |
|---|---|---|---|
| **OSV** (`api.osv.dev`) | the RustSec advisory data `cargo audit` reads, queried per crate + version | none | always |
| **cve.org** (`cveawg.mitre.org`) | the authoritative CVE record for each advisory alias — CVSS base score, publication state, title | none | always |
| **OpenCVE** (`app.opencve.io`) | a second view of the same CVE, useful for cross-checking a RUSTSEC-to-CVE mapping | `OPENCVE_API_KEY` (401 without it) | only when the key is exported |

The sync is **optional and offline-tolerant by design**. It is not a build step, not a test,
not a CI gate: nothing fails when it cannot run.

```bash
python3 scripts/security-sync.py                  # scan both manifests, print the findings
python3 scripts/security-sync.py --json out.json  # same, plus a machine-readable report
python3 scripts/security-sync.py --strict         # exit 1 on a finding, 3 if a query failed
OPENCVE_API_KEY=… python3 scripts/security-sync.py
```

- **Online:** prints the resolved package count per manifest and one line per advisory
  (RUSTSEC id, CVE alias, CVSS, fixed version, summary). Exit `0`; `1` with `--strict` when
  a finding exists.
- **Offline:** prints `offline: the recorded evaluation in SECURITY.md stands unchanged`,
  names the unreachable endpoint, and exits `0` — `3` with `--strict`, so a scheduled run
  can notice without turning an unreachable network into a failed build. The dated
  evaluation in this file is the fallback, which is why every entry carries a date.
- A findings run is a prompt to edit this document, not an automatic rewrite: the
  classification needs a human judgement about reachability that no scanner can make.

## What this evaluation does not cover

- **No `cargo-audit` binary** in the environment, and no `cargo-deny` license/advisory
  gate. The OSV query returns the same advisory data, but there is no CI step that fails a
  build on a finding — dependencies are audited when this document is regenerated.
- **No licence audit.** `cargo deny check licenses` is not run; the shipped licences have
  not been reviewed against a policy (there is no distribution policy yet).
- **No source review of transitive crates.** Presence or absence of a known advisory is
  the whole criterion; an unreported defect in a dependency is invisible here.
- **No supply-chain integrity beyond `Cargo.lock` + `--locked`**: crates.io checksums,
  no `cargo-vet`/`cargo-crev` review trail, no pinned registry mirror.

## Re-running the scan

The supported path is the script; the manual query below is what it automates.

```bash
# one advisory query per resolved package, both manifests
cargo tree --prefix none --format "{p}" | sed 's/ (\*)//' | \
  grep -E '^[A-Za-z0-9_-]+ v[0-9]' | sort -u > /tmp/pkgs.txt
cargo tree --manifest-path tools/screenshots/Cargo.toml --prefix none --format "{p}" | \
  sed 's/ (\*)//' | grep -E '^[A-Za-z0-9_-]+ v[0-9]' | sort -u > /tmp/pkgs-shots.txt
```

```python
# then, per file, POST the (name, version) pairs to https://api.osv.dev/v1/querybatch
# with ecosystem "crates.io"; any result with a vulns[] entry is a finding for this document
```

A finding lands here with its class, the crate that carries it, the path that reaches it,
whether this application can reach the vulnerable code, and the fixed version — and a
`risk` column in `DEPENDENCIES.md` carries the one-word verdict for the affected crate.

<!-- Last updated: 2026-11-19 · initial evaluation: 246 packages queried against OSV, RUSTSEC-2026-0486 (rustix xattr UB, medium, unreachable here) remediated by a 1.1.4 → 1.1.6 patch bump in both locks, RUSTSEC-2026-0192 (ttf-parser unmaintained, low) accepted as pinned by winit/sctk-adwaita with a re-check on winit upgrades. -->
