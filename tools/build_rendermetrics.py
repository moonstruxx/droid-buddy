#!/usr/bin/env python3
"""
Data factory for render-outlier-detection Track 2 (design D1/D3/D4).

Builds corpus/rendermetrics.csv: a deterministic feature matrix for
(patch, width, theme) triples plus an auditable degradation label.

Replica contract (design D4):
  - The INI parsing replicates Patch::from_ini_str (src/patch.rs):
    module-default expansion (a bare `[p2b8]` section declares B/L/P
    defaults), hardware-token scanning (scan_hw_tokens), LED association
    (`led = L.N` + `ledN` suffix pairing with same-suffix element entries),
    shift-group auto-assignment from the controller number
    `(leading_number(id) - 1) % 4`, and controller-panel assignment
    (CV I/O always "CV I/O"; known controller sections by name; else
    "Controller N" / "Other").
  - The feature math replicates src/rendermetrics.rs exactly:
    components/panels/modules counts (folded LEDs excluded), min_width
    (flat: n*16+2; subdivided: widest*16+4), overflow_cols,
    fallback_rate (cell_w < BOX_MIN_WIDTH over the 60%-panels-pane),
    sidebar/minimap visibility cascade (40-col source-pane floors,
    20-col remaining-content guards), min_contrast (minimum WCAG ratio
    of the co-occurring panel-surface tokens vs the assumed black
    background; Color::Reset tokens are unresolvable and skipped -> NA).

Labels (auditable, design D3):
    degraded = 1  iff  overflow_cols > 0 OR fallback_rate > 0
                       OR (min_contrast is not NA and min_contrast < 4.5)
    The three channels are exactly the renderer's degradation modes:
    native-fit clipping, boxed->unboxed cell fallback, and a palette
    contrast failure (the mono shift4=Black case the detector exists for).

Corpus (design D3): every fixtures/*.ini x widths [80,100,120] x themes
[classic,mono,terminal], plus injected known-bad rows at width 12 (forces
boxed->unboxed fallback on every panel, making the fallback axis
observable even though real terminals never get that narrow), plus
known-good rows at each patch's native-fit widths (min_width and
min_width+40, classic + terminal) so the clean class is well represented.
Mono contributes no clean rows by construction: its shift4=Black contrast
failure (ratio 1.0) is width-independent.

Determinism: features and labels are pure functions (byte-stable); the
only randomness is a row-order shuffle with random.Random(SEED=42),
mirroring tools/build_features.py's determinism contract. Running the
script twice produces identical bytes.

Run:  python3 tools/build_rendermetrics.py [--seed 42] [--check]
"""

from __future__ import annotations

import argparse
import csv
import random
import re
import sys
from pathlib import Path

SEED = 42
REPO = Path(__file__).resolve().parent.parent
FIXTURES_DIR = REPO / "fixtures"
CSV_PATH = REPO / "corpus" / "rendermetrics.csv"

# --- renderer layout constants (mirror src/ui.rs + src/rendermetrics.rs) ---
COMPONENT_WIDTH = 16
BOX_MIN_WIDTH = 8
MINIMAP_WIDTH = 3
PANELS_PERCENT = 60
SOURCE_PERCENT = 40
SIDEBAR_FLOOR_COLS = 40
CONTENT_MIN_COLS = 20
SIDEBAR_DIVISOR = 5
SIDEBAR_MIN_WIDTH = 20
DEFAULT_BG_RGB = (0, 0, 0)

# --- label threshold (WCAG AA for normal text) ---
CONTRAST_GOOD_FLOOR = 4.5

WIDTHS = [80, 100, 120]
INJECTED_WIDTH = 12  # forces cell_w < BOX_MIN_WIDTH on every panel
THEMES = ["classic", "mono", "terminal"]

HW_TOKEN_LETTERS = set("BLPOIESM")

# --- palettes (mirror src/theme.rs token values) ---
# (r, g, b) per ANSI-16 intended sRGB value; None == Color::Reset (terminal
# owns its colors -> unresolvable, skipped from contrast math).
ANSI16 = {
    "Black": (0, 0, 0),
    "Red": (255, 0, 0),
    "Green": (0, 255, 0),
    "Yellow": (255, 255, 0),
    "Blue": (0, 0, 255),
    "Magenta": (255, 0, 255),
    "Cyan": (0, 255, 255),
    "White": (255, 255, 255),
    "DarkGray": (128, 128, 128),
    "Gray": (192, 192, 192),
}

RESET = None  # Color::Reset


def palette(name: str) -> dict:
    """Token -> ANSI-16 color name (or RESET), mirroring Theme::classic/mono/terminal."""
    classic = {
        "button": "White", "switch": "White", "knob": "Magenta",
        "cv_in": "Cyan", "cv_out": "Green", "led": "Red",
        "shift1": "Yellow", "shift2": "Cyan", "shift3": "Magenta", "shift4": "Green",
        "muted": "DarkGray", "text": RESET,
    }
    mono = {
        "button": "White", "switch": "DarkGray", "knob": "White",
        "cv_in": "Gray", "cv_out": "Gray", "led": "White",
        "shift1": "Gray", "shift2": "White", "shift3": "DarkGray", "shift4": "Black",
        "muted": "DarkGray", "text": "White",
    }
    terminal = {k: RESET for k in classic}
    return {"classic": classic, "mono": mono, "terminal": terminal}[name]


# ---------------------------------------------------------------------------
# INI parsing replica (Patch::from_ini_str, src/patch.rs)
# ---------------------------------------------------------------------------

KNOWN_CONTROLLER_SECTIONS = {
    "notebuttons", "faderbank", "encoder", "pot",
    "unusedfaders", "motorfader", "fadermatrix",
    "p4b2", "p10", "s10", "p8s8", "b32", "e4", "m4", "db8e", "g8", "x7",
}

# Bare controller sections whose section name alone declares the full
# hardware token set (design.md Decision 2b, mirror of patch::BARE_SYNTHESIS):
# (section name, panel name, [(family letter, element count)]). Families and
# counts follow the manual: p8s8 = 8 sliders (P registers) + 8 slider LEDs +
# 8 switches, b32 = 32 buttons + 32 LEDs, m4 = 4 motor faders (P registers) +
# 4 touch buttons + 4 LEDs, e4 = 4 encoders + 4 encoder buttons + 4 ring LEDs.
BARE_SYNTHESIS = [
    ("p2b8", "P2B8", [("B", 8), ("L", 8), ("P", 2)]),
    ("p8s8", "P8S8", [("P", 8), ("L", 8), ("S", 8)]),
    ("b32", "B32", [("B", 32), ("L", 32)]),
    ("m4", "M4", [("P", 4), ("B", 4), ("L", 4)]),
    ("e4", "E4", [("E", 4), ("B", 4), ("L", 4)]),
    ("s10", "S10", [("S", 10)]),
    ("p10", "P10", [("P", 10)]),
]

# Component kind for a bare-synthesis family letter (mirror of
# patch::family_kind).
def family_kind(family: str) -> str:
    return {
        "B": "button", "L": "led", "P": "knob", "M": "knob",
        "S": "switch", "E": "encoder",
    }.get(family, "button")


def strip_comment(line: str) -> str:
    return line.split("#", 1)[0]


def parse_ini_sections(content: str):
    """Section list [{name (lower), entries: [(key_lower, value_raw)]}]."""
    sections = []
    for raw in content.splitlines():
        line = strip_comment(raw).strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            sections.append({"name": line[1:-1].strip().lower(), "entries": []})
            continue
        if "=" in line and line.count("=") >= 1:
            k, v = line.split("=", 1)
            k = k.strip().lower()
            v = v.strip()
            if k and sections:
                sections[-1]["entries"].append((k, v))
    return sections


def scan_hw_tokens(value: str):
    """Mirror scan_hw_tokens: HW letter + digit run, optional .digit run,
    boundary conditions (not preceded by alnum/underscore, clean end)."""
    tokens = []
    i, n = 0, len(value)
    while i < n:
        c = value[i]
        boundary_ok = i == 0 or not (value[i - 1].isalnum() or value[i - 1] == "_")
        starts = c in HW_TOKEN_LETTERS and i + 1 < n and value[i + 1].isdigit() and boundary_ok
        if starts:
            start = i
            i += 1
            while i < n and value[i].isdigit():
                i += 1
            if i < n and value[i] == "." and i + 1 < n and value[i + 1].isdigit():
                i += 1
                while i < n and value[i].isdigit():
                    i += 1
            clean_end = i >= n or not (value[i].isalnum() or value[i] == "_" or value[i] == ".")
            if clean_end:
                tokens.append(value[start:i])
            continue
        i += 1
    return tokens


def token_kind(token: str):
    """Mirror token_kind: B button, L led, P knob, O cvout, I cvin,
    E encoder, S switch, M knob (faders model as knobs)."""
    c = token[0] if token else ""
    return {
        "B": "button", "L": "led", "P": "knob", "O": "cvout",
        "I": "cvin", "E": "encoder", "S": "switch", "M": "knob",
    }.get(c)


def leading_number(ident: str):
    """Mirror leading_number: digit run after the first char (B1.1 -> 1)."""
    if len(ident) <= 1:
        return None
    m = re.match(r"\d+", ident[1:])
    return int(m.group(0)) if m else None


def leading_digits(s: str):
    """Mirror leading_digits: first digit run anywhere (button11 -> "11")."""
    m = re.search(r"\d+", s)
    return m.group(0) if m else None


def titlecase(s: str) -> str:
    return s[:1].upper() + s[1:]


def parse_patch_components(content: str):
    """Replicate from_ini_str's hardware-component construction.

    Returns list of dicts: id, kind, controller, shift_group (0..3 or None),
    led (str or None). Raises ValueError when no components are found.
    """
    sections = parse_ini_sections(content)
    if not sections:
        raise ValueError("No circuit sections found in patch file")

    components = []
    seen_ids = set()
    controller_chain_pos = 0
    pinned_panels = {}
    controller_types = {}

    def add_component(token, kind):
        if token in seen_ids:
            return False
        seen_ids.add(token)
        components.append({
            "id": token,
            "kind": kind,
            "controller": "",
            "shift_group": None,
            "led": None,
        })
        return True

    for section in sections:
        # LED association: `led = L.N` entry + first hardware token overall.
        led_token = next((v for k, v in section["entries"] if k == "led"), None)
        element_token = None
        for _, v in section["entries"]:
            tok = scan_hw_tokens(v)
            if tok:
                element_token = tok[0]
                break

        # A bare controller declaration implies its full hardware token set
        # even with no key-value pairs (design.md Decision 2b). The panel is
        # the controller type numbered by chain position; synthesized tokens
        # are pinned to that panel so they keep it even when another section
        # claimed the same controller number first.
        bare = next((p for p in BARE_SYNTHESIS if p[0] == section["name"]), None)
        if bare is not None:
            controller_chain_pos += 1
            n = controller_chain_pos
            controller_types.setdefault(n, bare[1])
            for family, count in bare[2]:
                for i in range(1, count + 1):
                    token = "%s%d.%d" % (family, n, i)
                    if add_component(token, family_kind(family)):
                        pinned_panels[token] = bare[1]
        elif section["name"] in KNOWN_CONTROLLER_SECTIONS:
            first_number = None
            for _, v in section["entries"]:
                for t in scan_hw_tokens(v):
                    num = leading_number(t)
                    if num is not None:
                        first_number = num
                        break
                if first_number is not None:
                    break
            if first_number is not None:
                panel = titlecase(section["name"])
                controller_types.setdefault(first_number, panel)
                # Pin every token this section declares to its own faceplate
                # (mirror of patch.rs): two explicit known types sharing a
                # controller number coexist on separate panels instead of the
                # later section's tokens falling through first-wins to the
                # earlier section's panel (droid_tui-26q). Skip tokens already
                # seen: a duplicate of a bare-synthesized token must not steal
                # the panel its synthesizing section pinned.
                for _key, value in section["entries"]:
                    for token in scan_hw_tokens(value):
                        if token_kind(token) is not None and token not in seen_ids:
                            pinned_panels[token] = panel

        for _key, value in section["entries"]:
            for token in scan_hw_tokens(value):
                kind = token_kind(token)
                if kind is not None:
                    add_component(token, kind)

        # Simple `led = L.N` association.
        if led_token is not None and element_token is not None:
            for comp in components:
                if comp["id"] == element_token:
                    comp["led"] = led_token
                    break

        # Numbered circuit LED params (led11 = L1.1) pair with the
        # same-suffix element entry (button11 = B1.1).
        element_by_suffix = {}
        for key, value in section["entries"]:
            if key.startswith("led"):
                continue
            suffix = leading_digits(key)
            if suffix is not None and token_kind(value) is not None:
                element_by_suffix.setdefault(suffix, value)
        for key, led in section["entries"]:
            if key.startswith("led"):
                suffix = leading_digits(key[len("led"):])
                if suffix is not None and suffix in element_by_suffix:
                    element = element_by_suffix[suffix]
                    for comp in components:
                        if comp["id"] == element:
                            comp["led"] = led
                            break

    if not components:
        raise ValueError("No hardware components found in patch file")

    # Shift groups by controller number (design.md Decision 2c).
    for comp in components:
        n = leading_number(comp["id"])
        if n is not None:
            comp["shift_group"] = (n - 1) % 4

    # Controller panels (design.md Decision 3): CV I/O tokens are fixed
    # jacks sharing one panel; bare-synthesized tokens keep the panel their
    # own section declared; others map via controller_types or "Controller N".
    for comp in components:
        if comp["kind"] in ("cvin", "cvout"):
            comp["controller"] = "CV I/O"
        elif comp["id"] in pinned_panels:
            comp["controller"] = pinned_panels[comp["id"]]
        else:
            n = leading_number(comp["id"])
            if n is not None and n in controller_types:
                comp["controller"] = controller_types[n]
            elif n is not None:
                comp["controller"] = "Controller %d" % n
            else:
                comp["controller"] = "Other"

    return components


# ---------------------------------------------------------------------------
# Feature extraction replica (RenderFeatures::extract, src/rendermetrics.rs)
# ---------------------------------------------------------------------------

def panel_models(comps):
    """Fold LEDs, order panels by first appearance, split into module groups
    (CV I/O never subdivides; one distinct instance -> flat)."""
    folded_led_ids = {c["led"] for c in comps if c["led"] is not None}
    folded_led_ids = {lid for lid in folded_led_ids
                      if any(c["id"] == lid and c["kind"] == "led" for c in comps)}

    by_panel = {}
    order = []
    for c in comps:
        if c["controller"] not in by_panel:
            by_panel[c["controller"]] = []
            order.append(c["controller"])
        by_panel[c["controller"]].append(c)

    models = []
    for name in order:
        visible = [c for c in by_panel[name]
                   if not (c["kind"] == "led" and c["id"] in folded_led_ids)]
        if name == "CV I/O":
            groups = [visible]
        else:
            by_instance = {}
            inst_order = []
            for c in visible:
                key = leading_number(c["id"]) or 0
                if key not in by_instance:
                    by_instance[key] = []
                    inst_order.append(key)
                by_instance[key].append(c)
            if len(inst_order) <= 1:
                groups = [visible]
            else:
                groups = [by_instance[k] for k in inst_order]
        models.append({"name": name, "visible": visible, "groups": groups})
    return models


def natural_width(model) -> int:
    if len(model["groups"]) > 1:  # subdivided
        widest = max((len(g) for g in model["groups"]), default=0)
        return widest * COMPONENT_WIDTH + 4
    return len(model["visible"]) * COMPONENT_WIDTH + 2


def fallback_rate(models, width) -> float:
    panels_area_w = width * PANELS_PERCENT // 100
    panels_pane_inner = max(0, panels_area_w - 2)
    unboxed = total = 0
    for model in models:
        grid_w = max(0, panels_pane_inner - (4 if len(model["groups"]) > 1 else 2))
        cell_w = min(COMPONENT_WIDTH, max(grid_w, 1))
        if cell_w < BOX_MIN_WIDTH:
            unboxed += len(model["visible"])
        total += len(model["visible"])
    return 0.0 if total == 0 else unboxed / total


def source_surface_decisions(patch, width):
    """Sidebar/minimap hidden flags, mirroring render_source_pane's cascade
    (order matters: sidebar floor -> sizing -> minimap floor -> remaining
    guards -> sidebar squeeze guard)."""
    source_w = width * SOURCE_PERCENT // 100
    has_sections = bool(patch["sections"])
    has_content = has_sections or bool(patch["raw_lines"])

    show_sidebar = source_w >= SIDEBAR_FLOOR_COLS and has_sections
    if show_sidebar:
        sidebar_w = max(source_w // SIDEBAR_DIVISOR, SIDEBAR_MIN_WIDTH)
        sidebar_w = min(sidebar_w, source_w - 20)
        if sidebar_w >= source_w:
            show_sidebar = False
            sidebar_w = 0
    else:
        sidebar_w = 0

    show_minimap = width >= 80 and source_w >= SIDEBAR_FLOOR_COLS and has_content
    remaining = source_w - sidebar_w - (MINIMAP_WIDTH if show_minimap else 0)
    if show_minimap and remaining < CONTENT_MIN_COLS:
        show_minimap = False
        remaining = source_w - sidebar_w
    if show_sidebar and remaining < CONTENT_MIN_COLS:
        show_sidebar = False
        remaining = source_w - (MINIMAP_WIDTH if show_minimap else 0)
        if remaining < CONTENT_MIN_COLS and show_minimap:
            show_minimap = False

    return (not show_sidebar, not show_minimap)


def channel_luminance(channel: int) -> float:
    s = channel / 255.0
    return s / 12.92 if s <= 0.03928 else ((s + 0.055) / 1.055) ** 2.4


def rgb_luminance(rgb) -> float:
    return 0.2126 * channel_luminance(rgb[0]) + 0.7152 * channel_luminance(rgb[1]) \
        + 0.0722 * channel_luminance(rgb[2])


def contrast_ratio(a: float, b: float) -> float:
    hi, lo = (a, b) if a >= b else (b, a)
    return (hi + 0.05) / (lo + 0.05)


def min_contrast(comps, theme_name):
    """Minimum WCAG ratio of co-occurring panel-surface tokens vs the assumed
    black background; None when no token resolves to RGB (terminal theme)."""
    theme = palette(theme_name)
    tokens = [theme["text"], theme["muted"]]
    kind_token = {
        "button": "button", "switch": "switch",
        "knob": "knob", "encoder": "knob",
        "led": "led", "cvin": "cv_in", "cvout": "cv_out",
    }
    for c in comps:
        tokens.append(theme[kind_token[c["kind"]]])
    if any(c["shift_group"] is not None for c in comps):
        tokens.extend([theme["shift1"], theme["shift2"], theme["shift3"], theme["shift4"]])

    bg_lum = rgb_luminance(DEFAULT_BG_RGB)
    ratios = []
    for tok in tokens:
        rgb = ANSI16.get(tok) if tok is not None else None
        if rgb is None:
            continue
        ratios.append(contrast_ratio(rgb_luminance(rgb), bg_lum))
    return min(ratios) if ratios else None


def extract_features(patch, width: int, theme_name: str) -> dict:
    models = panel_models(patch["components"])
    components = sum(len(m["visible"]) for m in models)
    panels = len(models)
    modules = sum(len(m["groups"]) for m in models)
    min_width = sum(natural_width(m) for m in models)
    overflow_cols = max(0, min_width - width)
    fb = fallback_rate(models, width)
    sidebar_hidden, minimap_hidden = source_surface_decisions(patch, width)
    mc = min_contrast(patch["components"], theme_name)
    return {
        "components": components,
        "panels": panels,
        "modules": modules,
        "min_width": min_width,
        "overflow_cols": overflow_cols,
        "fallback_rate": fb,
        "sidebar_hidden": 1 if sidebar_hidden else 0,
        "minimap_hidden": 1 if minimap_hidden else 0,
        "min_contrast": mc,
    }


def label_for(feat: dict) -> int:
    if feat["overflow_cols"] > 0:
        return 1
    if feat["fallback_rate"] > 0.0:
        return 1
    if feat["min_contrast"] is not None and feat["min_contrast"] < CONTRAST_GOOD_FLOOR:
        return 1
    return 0


# ---------------------------------------------------------------------------
# corpus assembly
# ---------------------------------------------------------------------------

def load_patches():
    patches = {}
    for path in sorted(FIXTURES_DIR.glob("*.ini")):
        try:
            comps = parse_patch_components(path.read_text())
        except ValueError as e:
            print(f"  skip {path.name}: {e}", file=sys.stderr)
            continue
        raw = path.read_text()
        patches[path.stem] = {
            "components": comps,
            "sections": parse_ini_sections(raw),
            "raw_lines": raw.splitlines(),
        }
    return patches


def build(seed: int, check: bool) -> int:
    print(f"build_rendermetrics: parsing fixtures -> {CSV_PATH.name} (seed {seed})")
    patches = load_patches()
    print(f"  {len(patches)} patches parsed")

    rows = []
    for name in sorted(patches):
        patch = patches[name]
        # min_width is theme-independent; compute once for the known-good rows.
        min_width = extract_features(patch, WIDTHS[0], THEMES[0])["min_width"]
        for width in WIDTHS:
            for theme in THEMES:
                feat = extract_features(patch, width, theme)
                rows.append({
                    "patch": name, "width": width, "theme": theme,
                    **feat, "degraded": label_for(feat),
                })
        for theme in THEMES:
            feat = extract_features(patch, INJECTED_WIDTH, theme)
            rows.append({
                "patch": name, "width": INJECTED_WIDTH, "theme": theme,
                **feat, "degraded": label_for(feat),
            })
        # Known-good rows at native fit: the renderer produces no fallback or
        # clipping at min_width (design D3's known-good definition).
        for width in [min_width, min_width + 40]:
            for theme in ["classic", "terminal"]:
                feat = extract_features(patch, width, theme)
                rows.append({
                    "patch": name, "width": width, "theme": theme,
                    **feat, "degraded": label_for(feat),
                })

    # random.Random(seed) determinism: shuffle the row order only.
    rng = random.Random(seed)
    rng.shuffle(rows)

    with open(CSV_PATH, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "patch", "width", "theme", "components", "panels", "modules",
            "min_width", "overflow_cols", "fallback_rate", "sidebar_hidden",
            "minimap_hidden", "min_contrast", "degraded",
        ])
        writer.writeheader()
        for row in rows:
            out = dict(row)
            mc = out["min_contrast"]
            out["min_contrast"] = "" if mc is None else "%.4f" % mc
            out["fallback_rate"] = "%.4f" % out["fallback_rate"]
            writer.writerow(out)

    n = len(rows)
    bad = sum(1 for r in rows if r["degraded"])
    print(f"  rows: {n}  (degraded {bad}, clean {n - bad}, "
          f"degraded share {bad / n:.2%})")
    print(f"  wrote {CSV_PATH}")

    if check:
        return self_check(patches)
    return 0


def self_check(patches) -> int:
    """Pin the replica to the Rust extractor's lead-verified expectations
    (src/rendermetrics.rs tests). Any mismatch -> exit 1."""
    checks = []

    def approx(a, b, tol=0.01):
        return a is not None and abs(a - b) < tol

    arp = patches["arpeggio1"]
    for theme, width, exp in [
        ("classic", 120, dict(components=14, panels=2, modules=2, min_width=228,
                              overflow_cols=108, fb=0.0, sb=False, mm=False)),
        ("classic", 100, dict(overflow_cols=128, sb=False, mm=True)),
        ("classic", 80, dict(overflow_cols=148, sb=True, mm=True)),
    ]:
        f = extract_features(arp, width, theme)
        for k, v in exp.items():
            got = {"fb": f["fallback_rate"], "sb": bool(f["sidebar_hidden"]),
                   "mm": bool(f["minimap_hidden"])}.get(k, f.get(k))
            checks.append((f"arpeggio {theme}@{width} {k}", got, v))
    checks.append(("arpeggio classic@120 min_contrast",
                   extract_features(arp, 120, "classic")["min_contrast"], 5.2520))
    checks.append(("arpeggio mono@120 min_contrast",
                   extract_features(arp, 120, "mono")["min_contrast"], 1.0))
    checks.append(("arpeggio terminal@120 min_contrast",
                   extract_features(arp, 120, "terminal")["min_contrast"], None))

    inf = patches["influence_outlier"]
    f = extract_features(inf, 120, "classic")
    checks.append(("influence_outlier classic@120 min_width", f["min_width"], 324))
    checks.append(("influence_outlier classic@120 components", f["components"], 20))

    mm = patches["multi_module_p2b8"]
    f = extract_features(mm, 120, "classic")
    checks.append(("multi_module classic@120 panels", f["panels"], 1))
    checks.append(("multi_module classic@120 modules", f["modules"], 2))
    checks.append(("multi_module classic@120 min_width", f["min_width"], 292))

    failures = 0
    for label, got, exp in checks:
        ok = (approx(got, exp) if exp is not None and got is not None
              else (got is None and exp is None) or (got == exp))
        if not ok:
            failures += 1
            print(f"  FAIL {label}: got {got!r}, expected {exp!r}")
    if failures:
        print(f"self-check: {failures} mismatch(es) — replica drifted from the Rust extractor")
        return 1
    print("self-check: replica matches the Rust extractor expectations")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=SEED)
    ap.add_argument("--check", action="store_true",
                    help="verify the replica against the Rust extractor's known values")
    args = ap.parse_args()
    return build(args.seed, args.check)


if __name__ == "__main__":
    sys.exit(main())