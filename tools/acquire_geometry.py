#!/usr/bin/env python3
"""Acquire + verify per-controller physical geometry for the DROID TUI.

Task 1.1 (physical-scale-model): encodes per-controller module geometry in mm
(1 HP = 5.08 mm, standard Eurorack) as `controller_geometry.json` at the repo
root, and cross-checks the result:

  (a) every controller type the embedded schema knows (ext/droid-lsp
      circuits.json -> "controllers") is present; all dimensions positive;
      every element cell lies inside its module rect; declared cell grids
      match token-family counts;
  (b) prints a summary table.

Sources: droid-manual-blue-7 (ch. 6 controllers, 7 G8, 8 X7, 9 MASTER18,
10 R2M/R2C, 14 hardware) + documented/reasonable pitch values where the manual
gives no millimetre positions (each controller carries a `notes` field naming
what is manual-documented vs. assumed). Unit is millimetres throughout.

Run:  python3 tools/acquire_geometry.py            # regenerate + verify + table
      python3 tools/acquire_geometry.py --check    # verify file on disk, no write
Stdlib only. Deterministic (no RNG, no external state).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# --- constants ---------------------------------------------------------------

HP_MM = 5.08
HE_MM = {1: 43.3, 3: 128.5}  # 1U (Intellijel) / 3U (standard Eurorack)
MARGIN_X = 2.54              # 0.1" side margin (faceplate inner area)
MARGIN_TOP = 12.0            # screw zone
MARGIN_BOT = 12.0

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "controller_geometry.json"
CIRCUITS = REPO / "ext" / "droid-lsp" / "droid-lsp" / "src" / "circuits.json"

TOL = 0.051  # mm equality tolerance (rounding to 2 decimals)

FAMILIES = {
    "B": "button",
    "P": "pot",
    "F": "fader",
    "E": "encoder",
    "S": "switch",
    "L": "LED",
    "CV": "jack (CV/gate)",
}

# schema-known controller types (mirrors circuits.json "controllers").
# Manual-documented modules outside the schema are appended below.
SCHEMA_TYPES = [
    "p2b8", "p4b2", "p8s8", "b32", "p10", "s10",
    "m4", "e4", "db8e", "x7",
]
MANUAL_TYPES = ["master", "master18", "g8", "r2m", "r2c"]

# --- helpers -----------------------------------------------------------------


def cell(col: int, row: int, x: float, y: float, w: float, h: float, **kw):
    d = {
        "col": col, "row": row,
        "x_mm": round(x, 2), "y_mm": round(y, 2),
        "w_mm": round(w, 2), "h_mm": round(h, 2),
    }
    d.update(kw)
    return d


def grid(cols, rows, x0, y0, px, py, w, h, order="row_wise", labels=None, **kw):
    """Regular grid of cells; index i -> (col,row) per `order`."""
    out = []
    for i in range(cols * rows):
        c, r = (i % cols, i // cols) if order == "row_wise" else (i // rows, i % rows)
        lab = labels[i] if labels else None
        d = cell(c, r, x0 + c * px, y0 + r * py, w, h, **kw)
        if lab is not None:
            d["label"] = lab
        out.append(d)
    return out


def ring_32(cx: float, cy: float, label0: int):
    """Square ring of 32 LEDs on the perimeter of a 9x9 grid (pitch 1.0 mm,
    outer half 4 mm) around encoder centre (cx, cy) — manual: 'a square of 32
    multicolor LEDs'. Returns 32 cells, labelled L{label0+1}..L{label0+32}."""
    out, n = [], 0
    for i in range(9):            # top
        out.append(cell(i, 0, cx - 4 + i * 1.0, cy - 4, 1, 1, label=f"L{label0 + n + 1}"))
        n += 1
    for j in range(1, 9):         # right
        out.append(cell(8, j, cx + 4, cy - 4 + j * 1.0, 1, 1, label=f"L{label0 + n + 1}"))
        n += 1
    for i in range(7, -1, -1):    # bottom
        out.append(cell(i, 8, cx - 4 + i * 1.0, cy + 4, 1, 1, label=f"L{label0 + n + 1}"))
        n += 1
    for j in range(7, 0, -1):     # left
        out.append(cell(0, j, cx - 4, cy - 4 + j * 1.0, 1, 1, label=f"L{label0 + n + 1}"))
        n += 1
    assert n == 32, "ring_32 must yield 32 LEDs"
    return out


def ctrl(name, he, width_hp, cells, token_counts, grids, notes):
    return {
        "he": he,
        "width_hp": width_hp,
        "width_mm": round(width_hp * HP_MM, 2),
        "height_mm": HE_MM[he],
        "token_counts": token_counts,
        "grids": grids,
        "element_cells": cells,
        "notes": notes,
    }


# --- per-controller layouts --------------------------------------------------

# Pitch constants (assumed where the manual is silent; see `notes`).
#   buttons     8x8 mm, pitch ~12.7 (0.5") or tighter on dense panels
#   big pot     12x12 mm, pitch 10 mm (P2B8/P4B2 class)
#   small pot   9x9 mm
#   jack        7x7 mm, pitch 8.5 mm x / 13 mm y
#   toggle      8x12 mm (3-position)
#   rotary      12x12 mm (8-position, S10 top row)
#   LED         3x3 mm standalone / 2x2 in-slider / 1x1 ring
#   encoder     7x7 mm (E4), 8x8 (DB8E)

CTRLS = {}


def _build():
    # --- master (8 HP) -------------------------------------------------------
    cells, grids = {}, []
    cells["L"] = grid(4, 4, 2.54, 20, 8.0, 8.0, 3, 3,
                      labels=[f"R{i}" for i in range(1, 17)])
    grids.append({"family": "L", "cols": 4, "rows": 4, "order": "row_wise",
                  "x0_mm": 2.54, "y0_mm": 20, "pitch_x_mm": 8.0, "pitch_y_mm": 8.0,
                  "w_mm": 3, "h_mm": 3})
    # reload button, right of the LED matrix
    cells["B"] = grid(1, 1, 31.0, 20, 0, 0, 7, 7, labels=["SD reload"])
    grids.append({"family": "B", "cols": 1, "rows": 1, "order": "row_wise",
                  "x0_mm": 31.0, "y0_mm": 20, "pitch_x_mm": 0, "pitch_y_mm": 0,
                  "w_mm": 7, "h_mm": 7})
    # CV: cols 0-1 inputs (I1..I8), cols 2-3 outputs (O1..O8), column-major
    cv = []
    for c in range(2):
        for r in range(4):
            cv.append(cell(c, r, 2.54 + c * 8.5, 60 + r * 13, 7, 7,
                           dir="in", label=f"I{c * 4 + r + 1}"))
    for c in range(2):
        for r in range(4):
            cv.append(cell(c + 2, r, 2.54 + (c + 2) * 8.5, 60 + r * 13, 7, 7,
                           dir="out", label=f"O{c * 4 + r + 1}"))
    cells["CV"] = cv
    grids.append({"family": "CV", "cols": 4, "rows": 4, "order": "col_major",
                  "x0_mm": 2.54, "y0_mm": 60, "pitch_x_mm": 8.5, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7,
                  "note": "cols 0-1 = inputs I1..I8, cols 2-3 = outputs O1..O8; "
                  "inputs numbered down the left column (I1..I4) then the right (I5..I8)"})
    CTRLS["master"] = ctrl(
        "master", 3, 8, cells,
        {"CV": 16, "L": 16, "B": 1}, grids,
        "Manual-documented: 8 HP; 8 CV in (I1..I8) + 8 CV out (O1..O8); "
        "16 full-color LEDs R1..R16 arranged 4x4, rows 1-2 = input LEDs, "
        "rows 3-4 = output LEDs (register table); SD reload button. "
        "Assumed: pitches/margins (LED pitch 8 mm, jack pitch 8.5x13 mm, "
        "0.1\" side margin, 12 mm screw zones).")

    # --- master18 (6 HP) ------------------------------------------------------
    cells, grids = {}, []
    cv = grid(2, 4, 1.5, 30, 8.5, 13, 7, 7, labels=[f"O{i}" for i in range(1, 9)],
              kind="cv_out")
    cv += grid(1, 2, 18.5, 30, 0, 13, 7, 7, labels=["Gin1", "Gin2"],
               kind="gate_in")
    cv += grid(1, 4, 18.5, 56, 0, 13, 7, 7,
               labels=["Gout1", "Gout2", "Gout3", "Gout4"], kind="gate_out")
    cells["CV"] = cv
    grids.append({"family": "CV", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 1.5, "y0_mm": 30, "pitch_x_mm": 8.5, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7, "kind": "cv_out"})
    grids.append({"family": "CV", "cols": 1, "rows": 2, "order": "row_wise",
                  "x0_mm": 18.5, "y0_mm": 30, "pitch_x_mm": 0, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7, "kind": "gate_in"})
    grids.append({"family": "CV", "cols": 1, "rows": 4, "order": "row_wise",
                  "x0_mm": 18.5, "y0_mm": 56, "pitch_x_mm": 0, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7, "kind": "gate_out"})
    cells["B"] = grid(1, 1, 22.5, 20, 0, 0, 7, 7, labels=["SD reload"])
    grids.append({"family": "B", "cols": 1, "rows": 1, "order": "row_wise",
                  "x0_mm": 22.5, "y0_mm": 20, "pitch_x_mm": 0, "pitch_y_mm": 0,
                  "w_mm": 7, "h_mm": 7})
    CTRLS["master18"] = ctrl(
        "master18", 3, 6, cells,
        {"CV": 14, "B": 1}, grids,
        "Manual-documented: 6 HP; 8 CV out (O1..O8), 2 gate inputs, 4 gate "
        "outputs (G1.1..G1.4 on the master18, see G8 chapter register "
        "numbering), MicroSD reload button. No LEDs (manual lists none). "
        "Assumed: pitches/margins; gate in/out jack grouping (right column).")

    # --- g8 (4 HP) -----------------------------------------------------------
    cells, grids = {}, []
    cells["CV"] = grid(2, 4, 2.0, 30, 7.5, 13, 7, 7,
                       dir="inout", labels=[f"G1.{i}" for i in range(1, 9)])
    grids.append({"family": "CV", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 2.0, "y0_mm": 30, "pitch_x_mm": 7.5, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7, "note": "tristate gate jacks (input or output)"})
    cells["L"] = grid(2, 4, 2.0, 22, 7.5, 13, 2, 2,
                      labels=[f"R{16 + i}" for i in range(1, 9)])
    grids.append({"family": "L", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 2.0, "y0_mm": 22, "pitch_x_mm": 7.5, "pitch_y_mm": 13,
                  "w_mm": 2, "h_mm": 2,
                  "note": "one LED above each jack; R17..R48 for G8s 1-4 (offset by chain slot)"})
    CTRLS["g8"] = ctrl(
        "g8", 3, 4, cells,
        {"CV": 8, "L": 8}, grids,
        "Manual-documented: 4 HP; 8 tristate gate jacks (G registers, dot "
        "notation per chain slot), 8 multicolor LEDs (R17..R24 first G8). "
        "Assumed: pitches/margins; LED above each jack.")

    # --- x7 (4 HP) -----------------------------------------------------------
    cells, grids = {}, []
    cells["S"] = grid(1, 1, 8.2, 12, 0, 0, 5, 14, labels=["USB"])
    grids.append({"family": "S", "cols": 1, "rows": 1, "order": "row_wise",
                  "x0_mm": 8.2, "y0_mm": 12, "pitch_x_mm": 0, "pitch_y_mm": 0,
                  "w_mm": 5, "h_mm": 14,
                  "note": "3-position USB switch (left SD / middle off / right MIDI)"})
    cells["CV"] = grid(1, 4, 6.66, 40, 0, 13, 7, 7, dir="out",
                       labels=[f"G{i}" for i in range(9, 13)])
    grids.append({"family": "CV", "cols": 1, "rows": 4, "order": "row_wise",
                  "x0_mm": 6.66, "y0_mm": 40, "pitch_x_mm": 0, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7, "note": "gate outputs G9..G12 (master); "
                  "G1.5..G1.8 with a master18"})
    cells["L"] = grid(2, 4, 13.5, 40, 3.0, 13, 1, 1,
                      labels=[f"R{48 + i}" for i in range(1, 9)])
    grids.append({"family": "L", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 13.5, "y0_mm": 40, "pitch_x_mm": 3.0, "pitch_y_mm": 13,
                  "w_mm": 1, "h_mm": 1,
                  "note": "R49..R56 for X7s 1-4 (offset by chain slot)"})
    CTRLS["x7"] = ctrl(
        "x7", 3, 4, cells,
        {"CV": 4, "L": 8, "S": 1}, grids,
        "Manual-documented: 4 HP; 7 ports (USB-C, MIDI TRS in/out, 4 gate "
        "outputs G9..G12), 3-position USB switch, 8 LEDs (R49..R56). "
        "USB-C + MIDI TRS jacks are physical ports, not patch-addressable "
        "cells — noted, not modelled. Assumed: pitches/margins.")

    # --- r2m / r2c (2 HP each) ------------------------------------------------
    for name in ("r2m", "r2c"):
        cells = {"CV": grid(1, 2, 1.58, 45, 0, 13, 7, 7, dir="serial",
                            labels=["serial in", "serial out"])}
        grids = [{"family": "CV", "cols": 1, "rows": 2, "order": "row_wise",
                  "x0_mm": 1.58, "y0_mm": 45, "pitch_x_mm": 0, "pitch_y_mm": 13,
                  "w_mm": 7, "h_mm": 7,
                  "note": "3.5 mm serial bridge jacks — transparent to patches, no tokens"}]
        CTRLS[name] = ctrl(
            name, 3, 2, cells, {"CV": 2}, grids,
            "Manual-documented: 2 HP; 2 serial line drivers (3.5 mm stereo "
            "patch-cable bridge, transparent to patches — no hardware tokens "
            "exist). Assumed: pitches/margins.")

    # --- p2b8 (5 HP) -----------------------------------------------------------
    cells, grids = {}, []
    cells["P"] = grid(2, 1, 2.54, 14, 10, 0, 12, 12, labels=["P1.1", "P1.2"],
                      size="big")
    grids.append({"family": "P", "size": "big", "cols": 2, "rows": 1,
                  "order": "row_wise", "x0_mm": 2.54, "y0_mm": 14,
                  "pitch_x_mm": 10, "pitch_y_mm": 0, "w_mm": 12, "h_mm": 12})
    cells["B"] = grid(2, 4, 3.5, 45, 9.2, 15, 8, 8,
                      labels=[f"B1.{i}" for i in range(1, 9)])
    grids.append({"family": "B", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 3.5, "y0_mm": 45, "pitch_x_mm": 9.2, "pitch_y_mm": 15,
                  "w_mm": 8, "h_mm": 8})
    cells["L"] = grid(2, 4, 6.0, 47.5, 9.2, 15, 3, 3,
                      labels=[f"L1.{i}" for i in range(1, 9)])
    grids.append({"family": "L", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 6.0, "y0_mm": 47.5, "pitch_x_mm": 9.2, "pitch_y_mm": 15,
                  "w_mm": 3, "h_mm": 3, "note": "LED inside each button"})
    CTRLS["p2b8"] = ctrl(
        "p2b8", 3, 5, cells,
        {"P": 2, "B": 8, "L": 8}, grids,
        "Manual-documented: 5 HP; two pots P1.1..P1.2, eight buttons B1.1.."
        "B1.8 with LEDs L1.1..L1.8. Assumed: pitches/margins (pot pitch "
        "10 mm, button pitch 9.2x15 mm); row-wise button numbering.")

    # --- p4b2 (5 HP) -----------------------------------------------------------
    cells, grids = {}, []
    cells["P"] = grid(2, 2, 2.54, 14, 10, 15, 12, 12,
                      labels=[f"P1.{i}" for i in range(1, 5)], size="big")
    grids.append({"family": "P", "size": "big", "cols": 2, "rows": 2,
                  "order": "row_wise", "x0_mm": 2.54, "y0_mm": 14,
                  "pitch_x_mm": 10, "pitch_y_mm": 15, "w_mm": 12, "h_mm": 12})
    cells["B"] = grid(2, 1, 3.5, 55, 9.2, 0, 8, 8, labels=["B1.1", "B1.2"])
    grids.append({"family": "B", "cols": 2, "rows": 1, "order": "row_wise",
                  "x0_mm": 3.5, "y0_mm": 55, "pitch_x_mm": 9.2, "pitch_y_mm": 0,
                  "w_mm": 8, "h_mm": 8})
    cells["L"] = grid(2, 1, 6.0, 57.5, 9.2, 0, 3, 3, labels=["L1.1", "L1.2"])
    grids.append({"family": "L", "cols": 2, "rows": 1, "order": "row_wise",
                  "x0_mm": 6.0, "y0_mm": 57.5, "pitch_x_mm": 9.2, "pitch_y_mm": 0,
                  "w_mm": 3, "h_mm": 3, "note": "LED inside each button"})
    CTRLS["p4b2"] = ctrl(
        "p4b2", 3, 5, cells,
        {"P": 4, "B": 2, "L": 2}, grids,
        "Manual-documented: 5 HP; four pots P1.1..P1.4, two buttons B1.1.."
        "B1.2 with LEDs. Assumed: pitches/margins (same pot class as P2B8).")

    # --- p10 (5 HP) ------------------------------------------------------------
    cells, grids = {}, []
    cells["P"] = grid(2, 1, 2.54, 14, 10, 0, 12, 12, labels=["P1.1", "P1.2"],
                      size="big")
    grids.append({"family": "P", "size": "big", "cols": 2, "rows": 1,
                  "order": "row_wise", "x0_mm": 2.54, "y0_mm": 14,
                  "pitch_x_mm": 10, "pitch_y_mm": 0, "w_mm": 12, "h_mm": 12})
    cells["P"] += grid(2, 4, 3.5, 42, 9.2, 15, 9, 9,
                       labels=[f"P1.{i}" for i in range(3, 11)], size="small")
    grids.append({"family": "P", "size": "small", "cols": 2, "rows": 4,
                  "order": "row_wise", "x0_mm": 3.5, "y0_mm": 42,
                  "pitch_x_mm": 9.2, "pitch_y_mm": 15, "w_mm": 9, "h_mm": 9})
    CTRLS["p10"] = ctrl(
        "p10", 3, 5, cells,
        {"P": 10}, grids,
        "Manual-documented: 5 HP; two big pots (P2B8 class, P1.1..P1.2) and "
        "eight small pots P1.3..P1.10. Assumed: pitches/margins; row-wise "
        "numbering (faceplate text-image shows 3..10 in 2 columns).")

    # --- s10 (5 HP) ------------------------------------------------------------
    cells, grids = {}, []
    cells["S"] = grid(2, 1, 2.54, 14, 10, 0, 12, 12, labels=["S1.1", "S1.2"],
                      kind="rotary")
    grids.append({"family": "S", "kind": "rotary", "cols": 2, "rows": 1,
                  "order": "row_wise", "x0_mm": 2.54, "y0_mm": 14,
                  "pitch_x_mm": 10, "pitch_y_mm": 0, "w_mm": 12, "h_mm": 12,
                  "note": "8-position rotary switches"})
    cells["S"] += grid(2, 4, 3.5, 42, 9.2, 15, 8, 12,
                       labels=[f"S1.{i}" for i in range(3, 11)], kind="small")
    grids.append({"family": "S", "kind": "small", "cols": 2, "rows": 4,
                  "order": "row_wise", "x0_mm": 3.5, "y0_mm": 42,
                  "pitch_x_mm": 9.2, "pitch_y_mm": 15, "w_mm": 8, "h_mm": 12,
                  "note": "3-position toggle switches"})
    CTRLS["s10"] = ctrl(
        "s10", 3, 5, cells,
        {"S": 10}, grids,
        "Manual-documented: 5 HP; two 8-position rotary switches S1.1..S1.2 "
        "and eight 3-position switches S1.3..S1.10. Assumed: pitches/margins; "
        "row-wise numbering.")

    # --- p8s8 (8 HP) -----------------------------------------------------------
    cells, grids = {}, []
    cells["F"] = grid(8, 1, 2.54, 16, 4.35, 0, 4.2, 22,
                      labels=[f"P1.{i}" for i in range(1, 9)],
                      note="sliders addressed via P registers")
    grids.append({"family": "F", "cols": 8, "rows": 1, "order": "row_wise",
                  "x0_mm": 2.54, "y0_mm": 16, "pitch_x_mm": 4.35, "pitch_y_mm": 0,
                  "w_mm": 4.2, "h_mm": 22,
                  "note": "20 mm travel sliders (P registers); LED inside slider"})
    cells["L"] = grid(8, 1, 3.64, 17.1, 4.35, 0, 2, 2,
                      labels=[f"L1.{i}" for i in range(1, 9)])
    grids.append({"family": "L", "cols": 8, "rows": 1, "order": "row_wise",
                  "x0_mm": 3.64, "y0_mm": 17.1, "pitch_x_mm": 4.35, "pitch_y_mm": 0,
                  "w_mm": 2, "h_mm": 2, "note": "LED inside each slider cap"})
    cells["S"] = grid(8, 1, 2.54, 48, 4.35, 0, 5, 12,
                      labels=[f"S1.{i}" for i in range(1, 9)])
    grids.append({"family": "S", "cols": 8, "rows": 1, "order": "row_wise",
                  "x0_mm": 2.54, "y0_mm": 48, "pitch_x_mm": 4.35, "pitch_y_mm": 0,
                  "w_mm": 5, "h_mm": 12, "note": "3-position toggle switches"})
    CTRLS["p8s8"] = ctrl(
        "p8s8", 3, 8, cells,
        {"F": 8, "L": 8, "S": 8}, grids,
        "Manual-documented: 8 HP; eight Alpha sliders 20 mm travel (P1.1.."
        "P1.8, slider LEDs L1.1..L1.8 defaulting to position), eight 3-pos "
        "toggle switches below (S1.1..S1.8). Assumed: pitches/margins; "
        "slider pitch 4.35 mm (dense 1 HP-class packing).")

    # --- b32 (10 HP) ------------------------------------------------------------
    cells, grids = {}, []
    cells["B"] = grid(4, 8, 3.1, 12, 11.15, 12.8, 9, 9,
                      labels=[f"B1.{i}" for i in range(1, 33)])
    grids.append({"family": "B", "cols": 4, "rows": 8, "order": "row_wise",
                  "x0_mm": 3.1, "y0_mm": 12, "pitch_x_mm": 11.15,
                  "pitch_y_mm": 12.8, "w_mm": 9, "h_mm": 9,
                  "note": "4 cols x 8 rows per the manual faceplate text-image "
                  "(buttons labelled 1..32 row-wise; 'a column of eight buttons')"})
    cells["L"] = grid(4, 8, 6.1, 15, 11.15, 12.8, 3, 3,
                      labels=[f"L1.{i}" for i in range(1, 33)])
    grids.append({"family": "L", "cols": 4, "rows": 8, "order": "row_wise",
                  "x0_mm": 6.1, "y0_mm": 15, "pitch_x_mm": 11.15,
                  "pitch_y_mm": 12.8, "w_mm": 3, "h_mm": 3,
                  "note": "LED inside each button; 4 brightness levels only"})
    CTRLS["b32"] = ctrl(
        "b32", 3, 10, cells,
        {"B": 32, "L": 32}, grids,
        "Manual-documented: 10 HP; 32 buttons B1.1..B1.32 with LEDs L1.1.."
        "L1.32, faceplate labelled 1..32 as 4 cols x 8 rows (resolves the "
        "8x4 orientation conflict). Assumed: pitches/margins.")

    # --- e4 (6 HP) --------------------------------------------------------------
    cells, grids = {}, []
    cells["E"] = grid(4, 1, 0.9, 54, 7.0, 0, 7, 7,
                      labels=[f"E1.{i}" for i in range(1, 5)])
    grids.append({"family": "E", "cols": 4, "rows": 1, "order": "row_wise",
                  "x0_mm": 0.9, "y0_mm": 54, "pitch_x_mm": 7.0, "pitch_y_mm": 0,
                  "w_mm": 7, "h_mm": 7,
                  "note": "96-step encoders; reduced side margins (dense panel), "
                  "adjacent LED rings overlap by 1 mm"})
    # encoder push buttons (B registers) co-located in the encoder shafts
    cells["B"] = grid(4, 1, 2.9, 56, 7.0, 0, 3, 3,
                      labels=[f"B1.{i}" for i in range(1, 5)])
    grids.append({"family": "B", "cols": 4, "rows": 1, "order": "row_wise",
                  "x0_mm": 2.9, "y0_mm": 56, "pitch_x_mm": 7.0, "pitch_y_mm": 0,
                  "w_mm": 3, "h_mm": 3, "note": "push button integrated in each encoder"})
    cells["L"] = []
    for col in range(4):
        cx = 4.4 + col * 7.0  # encoder centre x
        cells["L"] += ring_32(cx, 57.5, col * 32)
    CTRLS["e4"] = ctrl(
        "e4", 3, 6, cells,
        {"E": 4, "B": 4, "L": 128}, grids,
        "Manual-documented: 6 HP; four 96-step encoders E1.1..E1.4 each "
        "surrounded by a square of 32 multicolor LEDs ('square of 32' — "
        "4 rings x 32 = 128), push buttons in the encoder shafts (B1.1.."
        "B1.4); L1.1..L1.128 (rings 1..32, 33..64, ... per encoder). "
        "Assumed: pitches/margins (dense panel, reduced side margins); "
        "ring = perimeter of a 9x9 LED grid, 1.125 mm pitch.")

    # --- m4 (14 HP) -------------------------------------------------------------
    cells, grids = {}, []
    cells["F"] = grid(4, 1, 2.54, 16, 17.0, 0, 7, 62,
                      labels=[f"P1.{i}" for i in range(1, 5)],
                      note="motor faders addressed via P registers")
    grids.append({"family": "F", "cols": 4, "rows": 1, "order": "row_wise",
                  "x0_mm": 2.54, "y0_mm": 16, "pitch_x_mm": 17.0, "pitch_y_mm": 0,
                  "w_mm": 7, "h_mm": 62,
                  "note": "ALPS 60 mm action motorized faders (P registers)"})
    cells["B"] = grid(4, 1, 2.54, 88, 17.0, 0, 12, 18,
                      labels=[f"B1.{i}" for i in range(1, 5)],
                      kind="touch")
    grids.append({"family": "B", "cols": 4, "rows": 1, "order": "row_wise",
                  "x0_mm": 2.54, "y0_mm": 88, "pitch_x_mm": 17.0, "pitch_y_mm": 0,
                  "w_mm": 12, "h_mm": 18, "kind": "touch",
                  "note": "touch plates below each fader (B registers)"})
    cells["L"] = grid(4, 1, 2.54, 110, 17.0, 0, 3, 3,
                      labels=[f"L1.{i}" for i in range(1, 5)])
    grids.append({"family": "L", "cols": 4, "rows": 1, "order": "row_wise",
                  "x0_mm": 2.54, "y0_mm": 110, "pitch_x_mm": 17.0, "pitch_y_mm": 0,
                  "w_mm": 3, "h_mm": 3,
                  "note": "RGB LED below each touch plate (L + R registers)"})
    CTRLS["m4"] = ctrl(
        "m4", 3, 14, cells,
        {"F": 4, "B": 4, "L": 4}, grids,
        "Manual-documented: 14 HP; four ALPS 60 mm motor faders (P1.1..P1.4), "
        "touch plate with integrated RGB LED below each fader (B1.1..B1.4, "
        "L1.1..L1.4, R1.1..R1.4). Assumed: pitches/margins (fader pitch 17 mm).")

    # --- db8e (6 HP) ------------------------------------------------------------
    cells, grids = {}, []
    cells["B"] = grid(2, 4, 2.0, 38, 10, 16, 8, 8,
                      labels=[f"B1.{i}" for i in range(1, 9)])
    grids.append({"family": "B", "cols": 2, "rows": 4, "order": "row_wise",
                  "x0_mm": 2.0, "y0_mm": 38, "pitch_x_mm": 10, "pitch_y_mm": 16,
                  "w_mm": 8, "h_mm": 8,
                  "note": "8 push buttons; no LEDs on the buttons (manual silent)"})
    cells["E"] = grid(1, 1, 21.4, 56, 0, 0, 8, 8, labels=["E1.1"])
    grids.append({"family": "E", "cols": 1, "rows": 1, "order": "row_wise",
                  "x0_mm": 21.4, "y0_mm": 56, "pitch_x_mm": 0, "pitch_y_mm": 0,
                  "w_mm": 8, "h_mm": 8, "note": "rotary encoder (same as E4)"})
    cells["L"] = ring_32(25.4, 60, 0)
    CTRLS["db8e"] = ctrl(
        "db8e", 3, 6, cells,
        {"B": 8, "E": 1, "L": 32}, grids,
        "Manual-documented: 6 HP; display (D, not rendered — non-goal), eight "
        "push buttons B1.1..B1.8, one rotary encoder E1.1 with an LED square "
        "of 32 multicolor LEDs (same as E4, L1.1..L1.32). Assumed: pitches/"
        "margins; display area in the upper region is reserved, not modelled.")

    # --- chain gaps + defaults ---------------------------------------------------
    return CTRLS


# --- verification ----------------------------------------------------------------

def verify(ctrls, schema_types):
    """Returns (errors, warnings)."""

    def cell_in_rect(c, w, h):
        return (c["x_mm"] >= 0 and c["y_mm"] >= 0
                and c["w_mm"] > 0 and c["h_mm"] > 0
                and c["x_mm"] + c["w_mm"] <= w + TOL
                and c["y_mm"] + c["h_mm"] <= h + TOL)

    errors, warnings = [], []

    # (a) coverage of every schema-known controller type
    for t in schema_types:
        if t not in ctrls:
            errors.append(f"missing controller type '{t}' (schema-known)")
    extras = sorted(set(ctrls) - set(schema_types))
    if extras:
        warnings.append(f"extra controller types beyond schema: {extras} "
                        "(manual-documented modules)")

    for name, c in ctrls.items():
        if c["he"] not in (1, 3):
            errors.append(f"[{name}] he must be 1|3, got {c['he']}")
        if c["width_hp"] < 1:
            errors.append(f"[{name}] width_hp must be >= 1")
        if abs(c["width_mm"] - c["width_hp"] * HP_MM) > TOL:
            errors.append(f"[{name}] width_mm != width_hp * 5.08 "
                          f"({c['width_mm']} vs {c['width_hp'] * HP_MM})")
        if c["height_mm"] != HE_MM[c["he"]]:
            errors.append(f"[{name}] height_mm != HE_MM[{c['he']}]")

        # every cell: positive, inside module rect, unique label per family
        seen = {}
        addr_count = {}
        for fam, cells in c["element_cells"].items():
            if fam not in FAMILIES:
                errors.append(f"[{name}] unknown family '{fam}'")
            addr = 0
            for c_ in cells:
                if not cell_in_rect(c_, c["width_mm"], c["height_mm"]):
                    errors.append(f"[{name}] cell {fam}{c_.get('label', '')} "
                                  f"out of module rect: {c_}")
                lab = c_.get("label")
                if lab is not None:
                    if lab in seen:
                        errors.append(f"[{name}] duplicate label '{lab}'")
                    seen[lab] = True
                if c_.get("addressable", True):
                    addr += 1
            addr_count[fam] = addr

        # token_counts match addressable cell counts
        for fam, n in c["token_counts"].items():
            got = addr_count.get(fam, 0)
            if got != n:
                errors.append(f"[{name}] token_counts[{fam}] = {n} but "
                              f"{got} addressable cells present")
        for fam in addr_count:
            if fam not in c["token_counts"]:
                errors.append(f"[{name}] family '{fam}' has cells but no "
                              "token_counts entry")

        # declared grids: dims sum per (family,size,kind) == token count;
        # cells conform to grid positions (single grid per group)
        from collections import defaultdict
        per_fam = defaultdict(list)
        for g in c.get("grids", []):
            per_fam[(g["family"], g.get("size"), g.get("kind"))].append(g)
        for (fam, size, kind), gs in per_fam.items():
            if fam not in c["element_cells"]:
                errors.append(f"[{name}] grid family '{fam}' has no cells")
                continue
            cells = [c_ for c_ in c["element_cells"][fam]
                     if c_.get("size") == size and c_.get("kind") == kind]
            n = sum(g["cols"] * g["rows"] for g in gs)
            if n != len(cells):
                errors.append(f"[{name}] grids {fam} {size or ''} {kind or ''} "
                              f"declare {n} cells but {len(cells)} present")
                continue
            if len(gs) != 1:
                continue  # grouped grids share cells; per-grid order not asserted
            g = gs[0]
            for i, c_ in enumerate(cells):
                exp_c, exp_r = (i % g["cols"], i // g["cols"]) \
                    if g["order"] == "row_wise" else (i // g["rows"], i % g["rows"])
                exp_x = g["x0_mm"] + exp_c * g["pitch_x_mm"]
                exp_y = g["y0_mm"] + exp_r * g["pitch_y_mm"]
                if (c_["col"], c_["row"]) != (exp_c, exp_r):
                    errors.append(f"[{name}] {fam} cell {i} at "
                                  f"({c_['col']},{c_['row']}) expected "
                                  f"({exp_c},{exp_r})")
                if abs(c_["x_mm"] - exp_x) > TOL or abs(c_["y_mm"] - exp_y) > TOL:
                    errors.append(f"[{name}] {fam} cell {i} pos "
                                  f"({c_['x_mm']},{c_['y_mm']}) not on grid "
                                  f"({exp_x},{exp_y})")
                if abs(c_["w_mm"] - g["w_mm"]) > TOL or abs(c_["h_mm"] - g["h_mm"]) > TOL:
                    errors.append(f"[{name}] {fam} cell {i} size "
                                  f"({c_['w_mm']}x{c_['h_mm']}) != grid "
                                  f"({g['w_mm']}x{g['h_mm']})")
        # families with declared grids: sum of grid dims == token count
        fam_grid_total = defaultdict(int)
        for g in c.get("grids", []):
            fam_grid_total[g["family"]] += g["cols"] * g["rows"]
        for fam, n in fam_grid_total.items():
            if fam in c["token_counts"] and n != c["token_counts"][fam]:
                errors.append(f"[{name}] declared grids for '{fam}' sum to {n} "
                              f"but token_counts says {c['token_counts'][fam]}")

    return errors, warnings


def summary_table(ctrls):
    hdr = (f"{'type':<10}{'he':>3}{'HP':>5}{'WxH mm':>12}  "
           f"families (addressable cells)")
    rows = []
    for name in sorted(ctrls):
        c = ctrls[name]
        fam = ", ".join(f"{k} x{v}" for k, v in sorted(c["token_counts"].items()))
        rows.append(f"{name:<10}{c['he']:>3}{c['width_hp']:>5}"
                    f"{f'{c['width_mm']}x{c['height_mm']}':>12}  {fam}")
    return hdr + "\n" + "\n".join(rows)


def main():
    check_only = "--check" in sys.argv
    ctrls = _build()

    # schema-known types: read circuits.json when present, else the embedded list
    schema_types = list(SCHEMA_TYPES)
    if CIRCUITS.exists():
        with open(CIRCUITS) as f:
            data = json.load(f)
        known = sorted(data.get("controllers", {}).keys())
        if known:
            schema_types = known
    else:
        print(f"[warn] {CIRCUITS.relative_to(REPO)} not found — using "
              f"embedded schema list {schema_types}", file=sys.stderr)

    errors, warnings = verify(ctrls, schema_types)

    payload = {
        "meta": {
            "unit": "mm",
            "hp_mm": HP_MM,
            "he_mm": {str(k): v for k, v in HE_MM.items()},
            "families": FAMILIES,
            "chain_gaps_mm": {
                "inter_module": 0.5,
                "master_to_controller": 0.5,
            },
            "defaults": {"he": 3, "fallback_width_hp": 5,
                         "fallback_width_mm": round(5 * HP_MM, 2)},
            "source": ("droid-manual-blue-7 (ch. 6 controllers, 7 G8, 8 X7, "
                       "9 MASTER18, 10 R2M/R2C, 14 hardware) + documented/"
                       "reasonable pitch values where the manual gives no "
                       "millimetre positions; see per-controller `notes`"),
            "chain_gaps_note": ("assumption: Eurorack panels mount adjacent "
                                "with ~0.5 mm tolerance gap; the manual "
                                "specifies no fixed master->controller "
                                "spacing (ribbon cable, arbitrary length), so "
                                "the model renders the chain contiguously with "
                                "the inter-module gap"),
            "fallback_note": ("unknown/missing controller type falls back to "
                              "5 HP (average controller width) + warn, per "
                              "design D6"),
        },
        "controllers": ctrls,
    }

    if check_only:
        if not OUT.exists():
            print(f"FAIL: {OUT.relative_to(REPO)} missing", file=sys.stderr)
            sys.exit(1)
        with open(OUT) as f:
            on_disk = json.load(f)
        if on_disk != payload:
            print(f"FAIL: {OUT.relative_to(REPO)} differs from the generated "
                  "geometry (run without --check to regenerate)",
                  file=sys.stderr)
            sys.exit(1)
        if errors:
            for e in errors:
                print(f"ERROR: {e}", file=sys.stderr)
            sys.exit(1)
        print("OK: on-disk geometry matches generator and passes all checks")
        sys.exit(0)

    with open(OUT, "w") as f:
        json.dump(payload, f, indent=2, sort_keys=False)
        f.write("\n")

    print(summary_table(ctrls))
    print(f"\nchain_gaps_mm: {payload['meta']['chain_gaps_mm']}")
    print(f"defaults: {payload['meta']['defaults']}")
    print(f"wrote {OUT.relative_to(REPO)} "
          f"({len(ctrls)} controller types, "
          f"{sum(len(cs) for c in ctrls.values() for cs in c['element_cells'].values())} cells)")
    if warnings:
        for w in warnings:
            print(f"[warn] {w}", file=sys.stderr)
    if errors:
        for e in errors:
            print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)
    print("VERIFY: all checks passed")


if __name__ == "__main__":
    main()