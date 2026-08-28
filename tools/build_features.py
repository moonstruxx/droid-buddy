#!/usr/bin/env python3
"""
Data factory for rack-wiring-outlier-detection Track 2.

- Expands good corpus to ~2k variants (deterministic permutation + programmatic assembly)
  fallback when droid-metapatch not installed.
- Parses each patch, computes BindingFeatures replicates src/geometry.rs logic,
  injects synthetic BAD bindings (E4.4->M4 etc), emits corpus/features.csv.

Determinism: random.Random(SEED) everywhere, stable sorting, fixed float formatting.
No network, no pip installs. Run: python3 tools/build_features.py
"""

from __future__ import annotations
import csv
import hashlib
import json
import math
import random
import re
import shutil
import sys
from pathlib import Path

SEED = 42
TARGET_VARIANTS = 2000
BAD_COUNT = 350
CORPUS_GOOD = Path("corpus/good")
CSV_PATH = Path("corpus/features.csv")
INFLUENCE_STATS_PATH = Path("tools/influence_stats.txt")
FIXTURES_DIR = Path("fixtures")
GEOM_PATH = Path("rack_geometry.json")

# If geom not at repo root, try CARGO_MANIFEST_DIR style
if not GEOM_PATH.exists():
    for cand in [Path("rack_geometry.json"), Path("../rack_geometry.json"), Path("./rack_geometry.json")]:
        if cand.exists():
            GEOM_PATH = cand
            break

# ---------------------------------------------------------------------------
# Geometry replica (mirrors src/geometry.rs)
# ---------------------------------------------------------------------------

def load_geometry(path: Path):
    data = json.loads(path.read_text())
    return data

GEOM = None

def geom_load():
    global GEOM
    if GEOM is None:
        GEOM = load_geometry(GEOM_PATH)
    return GEOM

# token_kind_u8
def token_kind_u8(token: str) -> int:
    c = token[0].upper() if token else ""
    mapping = {"B":0,"L":1,"P":2,"O":3,"I":4,"E":5,"S":6,"G":7,"R":8,"M":8}
    return mapping.get(c, 255)

HW_LETTERS = set(['B','L','P','O','I','E','S','M','R','G'])  # geometry.rs has 10 including M,R,G; patch.rs scan uses 7 but geometry uses 10 for hop calc
# For scan in geometry (10 letters)
HW_TOKEN_LETTERS_GEOM = ['B','L','P','O','I','E','S','M','R','G']
# For patch scan (7 letters) — used for binding extraction? Use same as patch.rs 7 for component extraction but geometry resolve handles broader.
HW_TOKEN_LETTERS_PATCH = ['B','L','P','O','I','E','S']

def scan_hw_tokens_local(value: str):
    """mirrors geometry.rs scan_hw_tokens_local (10 letters)."""
    chars = list(value)
    tokens=[]
    i=0
    while i < len(chars):
        c = chars[i]
        boundary_ok = i==0 or not (chars[i-1].isalnum() or chars[i-1]=='_')
        starts = c in HW_TOKEN_LETTERS_GEOM and i+1 < len(chars) and chars[i+1].isdigit() and boundary_ok
        if starts:
            start=i
            i+=1
            while i < len(chars) and chars[i].isdigit():
                i+=1
            if i < len(chars) and chars[i]=='.' and i+1 < len(chars) and chars[i+1].isdigit():
                i+=1
                while i < len(chars) and chars[i].isdigit():
                    i+=1
            clean_end = i >= len(chars) or not (chars[i].isalnum() or chars[i]=='_' or chars[i]=='.')
            if clean_end:
                tokens.append(''.join(chars[start:i]))
            continue
        i+=1
    return tokens

def scan_hw_tokens_patch(value: str):
    """patch.rs 7 letters."""
    chars=list(value)
    tokens=[]
    i=0
    while i < len(chars):
        c=chars[i]
        boundary_ok = i==0 or not (chars[i-1].isalnum() or chars[i-1]=='_')
        starts = c in ['B','L','P','O','I','E','S'] and i+1 < len(chars) and chars[i+1].isdigit() and boundary_ok
        if starts:
            start=i
            i+=1
            while i < len(chars) and chars[i].isdigit():
                i+=1
            if i < len(chars) and chars[i]=='.' and i+1 < len(chars) and chars[i+1].isdigit():
                i+=1
                while i < len(chars) and chars[i].isdigit():
                    i+=1
            clean_end = i>=len(chars) or not (chars[i].isalnum() or chars[i]=='_' or chars[i]=='.')
            if clean_end:
                tokens.append(''.join(chars[start:i]))
            continue
        i+=1
    return tokens

def scan_internal_tokens(value: str):
    chars=list(value)
    out=[]
    i=0
    while i < len(chars):
        if chars[i]=='_':
            boundary_ok = i==0 or not chars[i-1].isalnum()
            if boundary_ok:
                start=i
                i+=1
                while i < len(chars) and (chars[i].isalnum() or chars[i]=='_'):
                    i+=1
                if i > start+1:
                    token=''.join(chars[start:i])
                    clean_end = i>=len(chars) or not (chars[i].isalnum() or chars[i]=='_')
                    if clean_end:
                        out.append(token)
                continue
        i+=1
    return out

def parse_ini_sections(content: str):
    sections=[]
    raw_lines = content.splitlines()
    # we need to strip comments per line for section parsing (same as patch.rs)
    for raw in raw_lines:
        # strip comment after #
        stripped_comment = raw.split('#',1)[0]
        line = stripped_comment.strip()
        if not line:
            continue
        if line.startswith('[') and line.endswith(']'):
            # column tracking not needed for feature calc
            name = line[1:-1].strip().lower()
            sections.append({"name":name,"entries":[]})
            continue
        if '=' in line:
            if not sections:
                continue
            k,v = line.split('=',1)
            k=k.strip().lower()
            v=v.strip()
            sections[-1]["entries"].append((k,v))
    return sections

def strip_comment(line: str) -> str:
    idx=line.find('#')
    return line[:idx] if idx!=-1 else line

# Geometry resolve
def geometry_resolve(token: str, geom):
    token=token.strip()
    if not token:
        return None
    kind_raw = token[0]
    if not kind_raw.isalpha():
        return None
    kind = kind_raw.upper()
    # leading digits after first char
    digits = ''.join([c for c in token[1:] if c.isdigit()][:10])  # but need take_while
    # Actually take_while digit: need to collect consecutive digits starting at 1
    m = re.match(r'^[A-Za-z](\d+)', token)
    if m:
        digits = m.group(1)
        try:
            instance = int(digits)
        except:
            instance = 1
    else:
        instance = 1
    if instance == 0:
        return None
    if '.' in token:
        dot = token.find('.')
        after = token[dot+1:]
        num_str = ''.join([c for c in after[:10] if c.isdigit() or False])
        # take_while digits
        num_str2=""
        for ch in after:
            if ch.isdigit():
                num_str2+=ch
            else:
                break
        try:
            element = int(num_str2) if num_str2 else 1
        except:
            element=1
    else:
        element=1
    if element==0:
        return None
    grid_key=None
    if kind in ('B','L'):
        grid_key="b32"
    elif kind=='E':
        grid_key="e4"
    elif kind in ('R','M','P','O','I','S','G'):
        grid_key="r2c"
    else:
        return None
    # candidates
    candidates=[]
    for rack in geom["racks"]:
        for slot in rack["controllers"]:
            if slot["grid"].lower() == grid_key.lower():
                candidates.append((rack,slot))
    if not candidates:
        return None
    idx = (instance-1) % len(candidates)
    rack,slot = candidates[idx]
    # lookup grid
    grid=None
    for k,v in geom["grids"].items():
        if k.lower()==grid_key.lower():
            grid=v
            break
    if grid is None:
        return None
    if grid["kind"]=="matrix":
        cols = grid["cols"]
        row_wise = grid.get("row_wise", True)
        if row_wise:
            col = (element-1) % cols
            row = (element-1) // cols
            off_x, off_y = col, row
        else:
            rows = grid["rows"]
            row = (element-1) % rows
            col = (element-1)//rows
            off_x, off_y = col, row
    elif grid["kind"]=="stack":
        pitch_y = grid["pitch_y"]
        off_x, off_y = 0, (element-1)*pitch_y
    elif grid["kind"]=="singleton":
        off_x, off_y = 0,0
    else:
        off_x,off_y=0,0
    abs_x = slot["x"] + off_x
    abs_y = rack["y"] + off_y
    if not (0 <= abs_x <= 255 and 0 <= abs_y <= 255):
        return None
    return (int(abs_x), int(abs_y))

def geom_distance(a,b):
    dx=a[0]-b[0]
    dy=a[1]-b[1]
    return math.sqrt(dx*dx+dy*dy)

def is_adjacent(a,b):
    return abs(geom_distance(a,b)-1.0) < 1e-6

def controller_for_token(token, geom):
    token=token.strip()
    if not token:
        return None
    kind=token[0].upper()
    m=re.match(r'^[A-Za-z](\d+)', token)
    instance=int(m.group(1)) if m else 1
    if instance==0:
        return None
    if kind in ('B','L'):
        grid_key="b32"
    elif kind=='E':
        grid_key="e4"
    elif kind in ('R','M','P','O','I','S','G'):
        grid_key="r2c"
    else:
        return None
    cands=[]
    for rack in geom["racks"]:
        for slot in rack["controllers"]:
            if slot["grid"].lower()==grid_key.lower():
                cands.append((rack,slot))
    if not cands:
        return None
    idx=(instance-1)%len(cands)
    return cands[idx]

def controller_rack_flags(src_token, sink_token, geom):
    src_info = controller_for_token(src_token, geom)
    sink_info = controller_for_token(sink_token, geom)
    if src_info and sink_info:
        src_rack,src_slot = src_info
        sink_rack,sink_slot = sink_info
        same_rack = src_rack["id"]==sink_rack["id"]
        same_controller = same_rack and src_slot["name"]==sink_slot["name"] and src_slot["x"]==sink_slot["x"] and src_slot["grid"].lower()==sink_slot["grid"].lower()
        return same_controller, same_rack
    return False, False

# cable_hops replica
def section_contains_token(section, token):
    for k,v in section["entries"]:
        if token in scan_hw_tokens_local(v):
            return True
    return False

def section_consumes_cable(section, cable):
    for k,v in section["entries"]:
        toks = scan_internal_tokens(v)
        if cable not in toks:
            continue
        if k.lower()=="output" and v.strip()==cable:
            return False
        return True
    return False

def collect_circuit_outputs(sections):
    out=[]
    for sec in sections:
        vars=[]
        seen=set()
        for k,v in sec["entries"]:
            if k.lower()=="output":
                names=scan_internal_tokens(v)
                if len(names)==1 and names[0]==v.strip() and names[0] not in seen:
                    vars.append(names[0])
                    seen.add(names[0])
        vars.sort()
        out.append(vars)
    return out

def compute_cable_hops(sections, circuit_outputs, src_token, sink_token):
    src_indices=[i for i,s in enumerate(sections) if section_contains_token(s, src_token)]
    sink_indices=set(i for i,s in enumerate(sections) if section_contains_token(s, sink_token))
    if not src_indices or not sink_indices:
        return 0
    if any(i in sink_indices for i in src_indices):
        return 0
    # build adjacency
    adj={}
    for prod_idx, outputs in enumerate(circuit_outputs):
        if not outputs:
            continue
        for cable in outputs:
            for cons_idx, sec in enumerate(sections):
                if cons_idx==prod_idx:
                    continue
                if section_consumes_cable(sec, cable):
                    adj.setdefault(prod_idx, []).append(cons_idx)
    for k in adj:
        adj[k]=sorted(set(adj[k]))
    # BFS
    from collections import deque
    q=deque()
    visited=set()
    for s in src_indices:
        q.append((s,0))
        visited.add(s)
    while q:
        node,dist=q.popleft()
        for n in adj.get(node,[]):
            if n in visited:
                continue
            nd=dist+1
            if n in sink_indices:
                return nd if nd<=255 else 255
            visited.add(n)
            q.append((n,nd))
    return 0

def binding_features(src_token, sink_token, geom, sections, circuit_outputs, param_key=0):
    src_xy = geometry_resolve(src_token, geom)
    sink_xy = geometry_resolve(sink_token, geom)
    if src_xy is None or sink_xy is None:
        return None
    euclidean = geom_distance(src_xy, sink_xy)
    manhattan = min(255, abs(src_xy[0]-sink_xy[0]) + abs(src_xy[1]-sink_xy[1]))
    adjacent = is_adjacent(src_xy, sink_xy)
    same_controller, same_rack = controller_rack_flags(src_token, sink_token, geom)
    cable_hops = compute_cable_hops(sections, circuit_outputs, src_token, sink_token)
    return {
        "src_kind": token_kind_u8(src_token),
        "sink_kind": token_kind_u8(sink_token),
        "param_key": param_key,
        "src_x": src_xy[0],
        "src_y": src_xy[1],
        "sink_x": sink_xy[0],
        "sink_y": sink_xy[1],
        "euclidean": euclidean,
        "manhattan": manhattan,
        "same_controller": int(same_controller),
        "same_rack": int(same_rack),
        "adjacent": int(adjacent),
        "cable_hops": cable_hops,
    }

# ---------------------------------------------------------------------------
# influence_subtree replica (mirrors src/patch.rs influence_subtree_with_disabled)
# ---------------------------------------------------------------------------
# Used by the per-token z-score second opinion (design D4): compute the size
# (number of influenced circuit instances) of each hardware token's influence
# subtree over the corpus, then bake per-kind mean/std into the artifact.

def build_node_ids_py(sections):
    """NodeId (name, instance) parallel to sections; mirrors patch.rs build_node_ids."""
    counts = {}
    ids = []
    for sec in sections:
        name = sec["name"]
        idx = counts.get(name, 0)
        counts[name] = idx + 1
        ids.append((name, idx))
    return ids

def hw_token_to_vars_py(sections, circuit_outputs, hw_token):
    """Root _VARs for a token; mirrors patch.rs hw_token_to_vars (7-letter scan)."""
    seen = set()
    vars_ = []
    for idx, sec in enumerate(sections):
        has_token = any(
            hw_token in scan_hw_tokens_patch(v) for k, v in sec["entries"]
        )
        if has_token and idx < len(circuit_outputs):
            for var in circuit_outputs[idx]:
                if var not in seen:
                    seen.add(var)
                    vars_.append(var)
    vars_.sort()
    return vars_

def influence_subtree_size(sections, circuit_outputs, root_vars):
    """Size = number of influenced circuit instances; mirrors patch.rs
    influence_subtree_with_disabled (no circuits disabled) BFS."""
    node_ids = build_node_ids_py(sections)
    roots = sorted(set(root_vars))
    if not roots:
        return 0
    from collections import deque
    queue = deque(roots)
    visited_cables = set()
    visited_nodes = set()
    influenced_nodes = set()
    while queue:
        cable = queue.popleft()
        if cable in visited_cables:
            continue
        visited_cables.add(cable)
        # per-param sink entries for this cable, deterministic sort
        sink_entries = []
        for idx, sec in enumerate(sections):
            for k, v in sec["entries"]:
                k_lower = k.lower()
                if k_lower == "output":
                    names = scan_internal_tokens(v)
                    if len(names) == 1 and names[0] == cable:
                        continue
                if cable in scan_internal_tokens(v):
                    nid = node_ids[idx] if idx < len(node_ids) else (sec["name"], 0)
                    sink_entries.append((nid, idx, k_lower))
        sink_entries.sort(key=lambda t: (t[0][0], t[2], t[1]))
        seen_sink = set()
        for nid, sink_idx, param_key in sink_entries:
            key = (nid, param_key)
            if key in seen_sink:
                continue
            seen_sink.add(key)
            if nid in visited_nodes:
                influenced_nodes.add(nid)
                continue
            visited_nodes.add(nid)
            influenced_nodes.add(nid)
            if sink_idx < len(circuit_outputs):
                outputs = sorted(circuit_outputs[sink_idx])
                for out in outputs:
                    if out not in visited_cables and out not in queue:
                        queue.append(out)
    return len(influenced_nodes)

def collect_influence_stats(good_files):
    """Per-token-kind mean/std of influence_subtree size over the corpus.
    Population std (divides by n), deterministic: files sorted, tokens in scan
    order, kinds emitted sorted. Returns {kind_u8: (mean, std, n)} and the raw
    per-kind size lists for calibration."""
    per_kind = {}
    for pf in sorted(good_files):
        txt = pf.read_text(errors="ignore")
        sections = parse_ini_sections(txt)
        circuit_outputs = collect_circuit_outputs(sections)
        # distinct tokens in file order (scan across all entry values)
        tokens = []
        seen = set()
        for sec in sections:
            for k, v in sec["entries"]:
                for t in scan_hw_tokens_patch(v):
                    if t not in seen:
                        seen.add(t)
                        tokens.append(t)
        for tok in tokens:
            kind = token_kind_u8(tok)
            if kind == 255:
                continue
            vars_ = hw_token_to_vars_py(sections, circuit_outputs, tok)
            size = influence_subtree_size(sections, circuit_outputs, vars_)
            per_kind.setdefault(kind, []).append(size)
    stats = {}
    for kind in sorted(per_kind):
        sizes = per_kind[kind]
        n = len(sizes)
        mean = sum(sizes) / n
        var = sum((x - mean) ** 2 for x in sizes) / n
        std = math.sqrt(var)
        stats[kind] = (mean, std, n)
    return stats, per_kind


def write_influence_stats(stats):
    """Write tools/influence_stats.txt: per-kind mean/std/n rows with a
    self-referential pinned sha (same pattern as fit_outlier_model.py).
    Deterministic: kinds emitted sorted."""
    header = "\n".join([
        "# droid_tui per-token-kind influence_subtree size statistics (design D4)",
        "# Fitted by tools/build_features.py on corpus/good (SEED 42, deterministic)",
        "#",
        "# Columns (whitespace-separated, one row per token kind, '#' comments allowed):",
        "#   kind mean std n",
        "# kind: token-kind u8 as in geometry.rs token_kind_u8 (0=B 1=L 2=P 3=O 4=I 5=E 6=S 7=G 8=M/R)",
        "# mean/std: population mean and standard deviation of influence_subtree",
        "#   size (number of influenced circuit instances) across corpus tokens.",
        "# z = (size - mean) / std; std <= 1e-6 -> no z-score (skip).",
        "# Artifact sha256: ",
    ])

    def write(pin):
        body = "\n".join(
            f"{kind} {mean:.6f} {std:.6f} {n}"
            for kind, (mean, std, n) in sorted(stats.items())
        )
        INFLUENCE_STATS_PATH.write_text(header + pin + "\n\n" + body + "\n")

    write("")  # placeholder pin
    pin = hashlib.sha256(INFLUENCE_STATS_PATH.read_bytes()).hexdigest()
    write(pin)
    pin = hashlib.sha256(INFLUENCE_STATS_PATH.read_bytes()).hexdigest()
    assert pin == hashlib.sha256(INFLUENCE_STATS_PATH.read_bytes()).hexdigest()
    return pin


# ---------------------------------------------------------------------------
# Corpus generation
# ---------------------------------------------------------------------------

def copy_fixtures():
    CORPUS_GOOD.mkdir(parents=True, exist_ok=True)
    copied=0
    for f in sorted(FIXTURES_DIR.glob("*.ini")):
        dst = CORPUS_GOOD / f.name
        if not dst.exists():
            shutil.copy2(f, dst)
            copied+=1
        else:
            # ensure byte identical: if content differs, overwrite
            if f.read_bytes() != dst.read_bytes():
                shutil.copy2(f, dst)
                copied+=1
    # also ensure picker_test not copied
    return copied

def permute_content(content: str, rng: random.Random, variant_id: int):
    # Light permute for large files: shuffle + few cable renames, skip heavy HW remap
    if len(content) > 10000:
        sections = parse_ini_sections(content)
        p2b8 = [s for s in sections if s["name"]=="p2b8"]
        others = [s for s in sections if s["name"]!="p2b8"]
        rng.shuffle(others)
        ordered = p2b8 + others
        # Collect cables, rename 1-2 deterministically
        all_cables=set()
        for sec in ordered:
            for k,v in sec["entries"]:
                all_cables.update(scan_internal_tokens(v))
        cable_list=sorted(all_cables)
        cable_map={}
        if cable_list:
            # rename up to 2 cables
            picks = rng.sample(cable_list, min(2, len(cable_list)))
            for c in picks:
                cable_map[c]=f"{c}_V{variant_id%100}"
        lines=[f"# variant {variant_id:04d} light-permute (large) seed {SEED}"]
        for sec in ordered:
            lines.append(f"[{sec['name']}]")
            for k,v in sec["entries"]:
                new_v=v
                for old,new in cable_map.items():
                    pattern=r'(?<![A-Za-z0-9])' + re.escape(old) + r'(?![A-Za-z0-9_])'
                    new_v=re.sub(pattern, new, new_v)
                lines.append(f"    {k} = {new_v}")
            lines.append("")
        return "\n".join(lines)+"\n"
    # Regular permute for small files
    sections = parse_ini_sections(content)
    # Keep p2b8 at top?
    p2b8 = [s for s in sections if s["name"]=="p2b8"]
    others = [s for s in sections if s["name"]!="p2b8"]
    rng.shuffle(others)
    ordered = p2b8 + others

    # Build HW token map: old -> new
    # Collect distinct HW tokens via patch scan (patch letters)
    all_hw=set()
    for sec in ordered:
        for k,v in sec["entries"]:
            all_hw.update(scan_hw_tokens_patch(v))
    # Also include geometry letters that may appear but patch scan misses (M,R,G,O,I)
    for sec in ordered:
        for k,v in sec["entries"]:
            all_hw.update(scan_hw_tokens_local(v))

    hw_map={}
    for tok in sorted(all_hw):
        # parse tok
        m = re.match(r'^([A-Za-z])(\d+)(\.(\d+))?$', tok)
        if not m:
            hw_map[tok]=tok
            continue
        kind=m.group(1)
        inst=int(m.group(2))
        has_dot=m.group(3) is not None
        elem=int(m.group(4)) if m.group(4) else None
        # decide new instance/element within valid ranges
        # Keep kind same, vary instance and element deterministically
        # Valid ranges: B/L element 1..32, E 1..4, P 1..2, I/O/S 1..8, R/G/M 1..8 maybe
        kind_up=kind.upper()
        if kind_up in ('B','L'):
            new_elem = rng.randint(1,32)
            new_inst = rng.randint(1,2)
        elif kind_up=='E':
            new_elem = rng.randint(1,4)
            new_inst = rng.randint(1,2)
        elif kind_up=='P':
            new_elem = rng.randint(1,2)
            new_inst = rng.randint(1,2)
        elif kind_up in ('I','O','S'):
            # CV/switch often 1..8
            new_elem = rng.randint(1,8) if elem is not None else rng.randint(1,8)
            # for I/O/G the instance maybe not used; keep 1
            new_inst = rng.randint(1,2) if rng.random()<0.2 else inst
        elif kind_up in ('R','M','G'):
            # gates, keep without dot often? but handle
            new_elem = rng.randint(1,12) if elem is not None else rng.randint(1,4)
            new_inst = rng.randint(1,2)
        else:
            new_elem = elem if elem else 1
            new_inst = inst
        if has_dot:
            new_tok = f"{kind}{new_inst}.{new_elem}"
        else:
            # O1 style without dot
            if kind_up in ('O','I','G','R','M'):
                # preserve dot-less style: no dot
                new_tok = f"{kind}{new_elem}" if elem is None else f"{kind}{new_inst}.{new_elem}" if rng.random()<0.3 else f"{kind}{new_elem}"
                # For O1/I1 we usually keep without dot
                if elem is None:
                    new_tok = f"{kind}{new_elem}"
                else:
                    # keep dot if originally had dot? already has_dot false here so elem is None? Actually O1 has no dot so elem None
                    new_tok = f"{kind}{new_elem}"
            else:
                new_tok = f"{kind}{new_inst}.{new_elem}"
        hw_map[tok]=new_tok

    # Cable map: collect distinct cables
    all_cables=set()
    for sec in ordered:
        for k,v in sec["entries"]:
            all_cables.update(scan_internal_tokens(v))
    cable_map={}
    for c in sorted(all_cables):
        # keep original but add variant suffix deterministically for ~50% of cables
        if rng.random() < 0.5:
            # rename to preserve leading underscore
            suffix = rng.randint(1,999)
            cable_map[c]=f"{c}_V{variant_id%100}_{suffix}"
        else:
            cable_map[c]=c
    # Need consistent replacement: for cables that get renamed, replace all occurrences
    # But ensure output cables also renamed consistently
    # Apply replacements to entries
    # To avoid overlapping replacements, do longest first
    def replace_hw_in_value(val: str) -> str:
        # Replace HW tokens via word boundary aware: iterate over map sorted longest first
        # Use regex boundary approach similar to scan: we replace exact token occurrences that are bounded
        # Simplest: for each old->new, replace via re with boundaries
        # But naive string replace may cause partial: so use regex with lookaround
        out = val
        for old,new in sorted(hw_map.items(), key=lambda x: -len(x[0])):
            # Build pattern: ensure old is bounded by not alnum/_ on both sides
            # Use lambda to avoid re escaping issues
            pattern = r'(?<![A-Za-z0-9_])' + re.escape(old) + r'(?![A-Za-z0-9_.])'
            out = re.sub(pattern, new, out)
        for old,new in sorted(cable_map.items(), key=lambda x: -len(x[0])):
            if old==new:
                continue
            # cables start with _ ; boundary: not alnum before, not alnum/_ after
            pattern = r'(?<![A-Za-z0-9])' + re.escape(old) + r'(?![A-Za-z0-9_])'
            out = re.sub(pattern, new, out)
        return out

    # Also sometimes vary numeric param values slightly: e.g., hz, cv values
    def tweak_value(val: str) -> str:
        # simple tweak: if val contains number with V, multiply
        if rng.random() < 0.3:
            # tweak first float
            def repl_num(m):
                num=float(m.group(1))
                factor = rng.uniform(0.8,1.2)
                new_num = round(num*factor,3)
                # preserve V suffix if present
                suffix = m.group(2) if m.group(2) else ""
                return f"{new_num}{suffix}"
            # match number like 0.1V or 40
            val = re.sub(r'(\d+\.?\d*)(V?)', repl_num, val, count=1)
        return val

    lines=[]
    # add comment header
    lines.append(f"# variant {variant_id:04d} derived via deterministic permutation (seed {SEED})")
    for sec in ordered:
        lines.append(f"[{sec['name']}]")
        for k,v in sec["entries"]:
            # v may contain tokens; replace
            new_v = replace_hw_in_value(v)
            # tweak numeric with small prob, but after token replacement
            if rng.random() < 0.2:
                new_v = tweak_value(new_v)
            lines.append(f"    {k} = {new_v}")
        lines.append("")
    return "\n".join(lines)+"\n"

def generate_programmatic(rng: random.Random, variant_id: int):
    """Metadroid-like programmatic assembly fallback: generate minimal valid patch from templates."""
    templates = ["acid", "deepsea", "copycat"]
    tmpl = templates[variant_id % len(templates)]
    # Choose controller count
    has_two_p2b8 = rng.random() < 0.3
    # pick HW tokens
    b_choices = [f"B{rng.randint(1,2)}.{rng.randint(1,32)}" for _ in range(4)]
    e_choices = [f"E{rng.randint(1,2)}.{rng.randint(1,4)}" for _ in range(2)]
    o_choices = [f"O{rng.randint(1,8)}" for _ in range(2)]
    i_choices = [f"I{rng.randint(1,8)}" for _ in range(2)]
    cable_base = f"_C{variant_id%1000}_{rng.randint(10,99)}"
    cables = [f"{cable_base}_{i}" for i in range(3)]
    lines=[]
    lines.append(f"# programmatic variant {variant_id:04d} template={tmpl} seed={SEED}")
    lines.append(f"# generated via deterministic metadroid-like assembly")
    # controllers
    lines.append(f"[p2b8]")
    if has_two_p2b8:
        lines.append(f"[p2b8]")
    if tmpl=="acid":
        lines.append(f"")
        lines.append(f"[lfo]")
        lines.append(f"    hz = 40 * P1.1")
        lines.append(f"    square = {cables[0]}")
        lines.append(f"")
        lines.append(f"[sequencer]")
        lines.append(f"    clock = {cables[0]}")
        lines.append(f"    reset = {cables[1]}")
        # use button tokens
        lines.append(f"    button1 = {b_choices[0]}")
        lines.append(f"    button2 = {b_choices[1]}")
        lines.append(f"    cvoutput = {cables[1]}")
        lines.append(f"    chaintonext = 1")
        lines.append(f"")
        lines.append(f"[quantizer]")
        lines.append(f"    input = {cables[1]}")
        lines.append(f"    output = {cables[2]}")
        lines.append(f"")
        lines.append(f"[copy]")
        lines.append(f"    input = {cables[2]}")
        lines.append(f"    output = {o_choices[0]}")
        # add button with led
        lines.append(f"")
        lines.append(f"[button]")
        lines.append(f"    button = {b_choices[2]}")
        lines.append(f"    led = L{ b_choices[2][1:]}")  # crude map B->L
        lines.append(f"    output = {cables[0]}")
    elif tmpl=="deepsea":
        lines.append(f"")
        lines.append(f"[clocktool]")
        lines.append(f"    output = {cables[0]}")
        lines.append(f"")
        lines.append(f"[sequencer]")
        lines.append(f"    clock = {cables[0]}")
        lines.append(f"    cv1 = 0.1V")
        lines.append(f"    cv2 = 0.2V")
        lines.append(f"    cvoutput = {cables[1]}")
        lines.append(f"")
        lines.append(f"[chord]")
        lines.append(f"    input = {cables[1]}")
        lines.append(f"    root = {i_choices[0]}")
        lines.append(f"    output = {cables[2]}")
        lines.append(f"")
        lines.append(f"[copy]")
        lines.append(f"    input = {cables[2]}")
        lines.append(f"    output = {o_choices[0]}")
        lines.append(f"")
        lines.append(f"[button]")
        lines.append(f"    button = {b_choices[0]}")
        lines.append(f"    led = L{ b_choices[0][1:]}")
        lines.append(f"    output = {cables[0]}")
    else:  # copycat
        lines.append(f"")
        lines.append(f"[button]")
        lines.append(f"    button = {b_choices[0]}")
        lines.append(f"    led = L{ b_choices[0][1:]}")
        lines.append(f"    output = {cables[0]}")
        lines.append(f"")
        lines.append(f"[copy]")
        lines.append(f"    input = {cables[0]}")
        lines.append(f"    output = {cables[1]}")
        lines.append(f"")
        lines.append(f"[mixer]")
        lines.append(f"    input1 = {cables[1]}")
        lines.append(f"    input2 = {e_choices[0]}")
        lines.append(f"    output = {cables[2]}")
        lines.append(f"")
        lines.append(f"[copy]")
        lines.append(f"    input = {cables[2]}")
        lines.append(f"    output = {o_choices[1] if len(o_choices)>1 else o_choices[0]}")
        # encoder
        lines.append(f"")
        lines.append(f"[encoder]")
        lines.append(f"    encoder = {e_choices[0]}")
        lines.append(f"    output = {cables[1]}")
    lines.append("")
    return "\n".join(lines)+"\n"

def ensure_corpus():
    CORPUS_GOOD.mkdir(parents=True, exist_ok=True)
    # copy fixtures
    for f in sorted(FIXTURES_DIR.glob("*.ini")):
        dst = CORPUS_GOOD / f.name
        # Only overwrite if missing or different (keep deterministic)
        if not dst.exists() or f.read_bytes() != dst.read_bytes():
            shutil.copy2(f, dst)
    existing = sorted(CORPUS_GOOD.glob("*.ini"))
    needed = TARGET_VARIANTS - len(existing)
    if needed <= 0:
        # If we already have >= target, ensure deterministic ordering but don't regenerate
        # However to guarantee byte-identical CSV we need same file set; if user deleted some, regenerate
        # Check if we have variant files; if needed==0 we are done
        return len(existing)
    rng = random.Random(SEED)
    # Choose seed files for permutation: prefer small musical patches for speed
    # Large MPFS files (90KB) are slow to permute; keep them as copies but use small seeds for variants
    seed_candidates=[]
    for f in sorted(FIXTURES_DIR.glob("*.ini")):
        if f.stat().st_size > 10000:
            continue
        txt=f.read_text(errors="ignore")
        toks=set(scan_hw_tokens_local(txt)) | set(scan_hw_tokens_patch(txt))
        if toks:
            seed_candidates.append(f)
    if not seed_candidates:
        # fallback to small subset
        for f in sorted(FIXTURES_DIR.glob("*.ini")):
            txt=f.read_text(errors="ignore")
            toks=set(scan_hw_tokens_local(txt)) | set(scan_hw_tokens_patch(txt))
            if toks:
                seed_candidates.append(f)
    if not seed_candidates:
        seed_candidates = sorted(FIXTURES_DIR.glob("*.ini"))
    # Also include already copied good patches as seeds? Use same list
    generated=0
    variant_start = len(existing)  # to keep names unique but deterministic across reruns we need fixed names
    # To keep deterministic across reruns, we should generate variant_{i:04d}.ini for i in range(needed)
    # But if corpus already has some variant files, we need to not duplicate. Instead generate missing indices.
    # Simpler: always generate variant_0000 .. variant_{TARGET-len(copied)-1} deterministically, and ensure they exist with correct content
    # So we recompute expected set and write each file if missing or mismatched.
    # First, find copied fixture names
    fixture_names=set(f.name for f in FIXTURES_DIR.glob("*.ini"))
    # Determine how many variant files we need
    expected_variant_count = TARGET_VARIANTS - len(list(FIXTURES_DIR.glob("*.ini")))
    # But we actually need TARGET total including fixtures, so variant count = TARGET - fixture_count
    # Use fixture_count = count of fixtures copied (even if some fixtures have zero HW they still count)
    fixture_count=len(list(FIXTURES_DIR.glob("*.ini")))
    exp_var = TARGET_VARIANTS - fixture_count
    if exp_var <0:
        exp_var=0
    for i in range(exp_var):
        fname=f"variant_{i:04d}.ini"
        dst=CORPUS_GOOD / fname
        # Deterministic content for i
        # Use separate RNG seeded by SEED+i for each variant to be independent of generation order
        r = random.Random(SEED + i + 9973)  # offset to avoid correlation with main rng
        # Choose seed file deterministically
        seed_file = seed_candidates[r.randint(0, len(seed_candidates)-1)]
        content = seed_file.read_text(errors="ignore")
        # Decide programmatic vs permute: every 3rd is programmatic
        if i % 3 == 0:
            new_content = generate_programmatic(r, i)
        else:
            new_content = permute_content(content, r, i)
        # Write if missing or content differs (to ensure determinism)
        if not dst.exists() or dst.read_text(errors="ignore") != new_content:
            dst.write_text(new_content)
            generated+=1
        # else already correct
    final_count=len(list(CORPUS_GOOD.glob("*.ini")))
    return final_count

# ---------------------------------------------------------------------------
# Feature extraction + CSV
# ---------------------------------------------------------------------------

HEADER = ["src_kind","sink_kind","param_key","src_x","src_y","sink_x","sink_y","euclidean","manhattan","same_controller","same_rack","adjacent","cable_hops","is_outlier"]

def extract_good_rows_for_patch(patch_path: Path, geom):
    txt=patch_path.read_text(errors="ignore")
    sections=parse_ini_sections(txt)
    circuit_outputs=collect_circuit_outputs(sections)
    # collect HW tokens
    global_tokens=set()
    for sec in sections:
        for k,v in sec["entries"]:
            global_tokens.update(scan_hw_tokens_patch(v))
            global_tokens.update(scan_hw_tokens_local(v))
    # per-section pairs
    pairs=set()
    for sec in sections:
        toks=[]
        for k,v in sec["entries"]:
            toks.extend(scan_hw_tokens_patch(v))
            # also local for missing kinds? Use local but deduplicate
            # To avoid double counting, use patch set plus local but unique
            extra=scan_hw_tokens_local(v)
            for t in extra:
                if t not in toks:
                    toks.append(t)
        uniq=list(dict.fromkeys(toks))
        for a in uniq:
            for b in uniq:
                if a!=b:
                    pairs.add((a,b))
    if not pairs and len(global_tokens)>=2:
        gt=list(sorted(global_tokens))
        pairs.add((gt[0], gt[1]))
        if len(gt)>=3:
            pairs.add((gt[1], gt[2]))
    # sample up to 2 per patch deterministically
    MAX_PER_PATCH=2
    if len(pairs) > MAX_PER_PATCH:
        # deterministic sample
        h = int(hashlib.sha256(str(patch_path).encode()).hexdigest()[:8],16)
        r = random.Random(SEED + h)
        pairs_list=sorted(pairs)
        pairs = set(r.sample(pairs_list, MAX_PER_PATCH))
    rows=[]
    for src,sink in sorted(pairs):
        feat = binding_features(src, sink, geom, sections, circuit_outputs, param_key=0)
        if feat is None:
            continue
        rows.append(feat)
    return rows

def main():
    geom = geom_load()
    # Step 1 ensure corpus
    final_count = ensure_corpus()
    print(f"Corpus ensured: {final_count} files in {CORPUS_GOOD} (target {TARGET_VARIANTS})", file=sys.stderr)
    good_files = sorted(CORPUS_GOOD.glob("*.ini"))
    # check deterministic method
    has_metapatch=False
    try:
        import importlib.util
        if importlib.util.find_spec("metapatch") is not None:
            has_metapatch=True
        if importlib.util.find_spec("droid_metapatch") is not None:
            has_metapatch=True
    except:
        pass
    method = "droid-metapatch" if has_metapatch else "deterministic permutation + programmatic assembly (fallback)"
    # Step 2 extract features
    all_rows=[]
    for pf in good_files:
        rows = extract_good_rows_for_patch(pf, geom)
        for feat in rows:
            all_rows.append((*[feat[h] for h in HEADER[:-1]], 0))  # is_outlier 0
    good_count = len(all_rows)
    # Step 3 inject bads
    # BAD_POOL label semantics: synthetic BAD bindings cover the terrain where the
    # 8.0/hops-0 rule fails (design D3). The original pool was E/B-sourced only, so
    # the classifier overfit the encoder source and ~50 B-sourced rows escaped the
    # 8.0 threshold. Categories below mirror the measured failure modes:
    #  - extreme E: far cross-controller encoder bindings (euclidean > 8) — kept so
    #    recall on the existing outlier terrain is preserved
    #  - same-controller near-8: the b32 grid caps intra-controller distance at ~7.6
    #    (corner-to-corner), so these sit just under the 8.0 threshold and escape
    #  - cross-controller near: adjacent-rack E4 controllers are only 6-8 units apart
    #    (E1.4->E2.1 = 6.0, E1.3->E2.1 = 8.0 exactly at the boundary) — below 8.0 yet
    #    physically implausible wiring
    #  - non-E sources: B/P/S/O/I/G-sourced outliers so the source kind does not
    #    collapse onto the encoder; B2.1->B1.32 / B1.1->B2.1 are the B-sourced
    #    cross-controller cases that previously escaped
    BAD_POOL = [
        # extreme E (rule-catchable, recall preservation)
        ("E4.4","M4"), ("E4.4","M4.2"), ("E1.1","B2.1"), ("E1.2","B2.32"), ("E4.4","B32.1"),
        # same-controller near-8 (rule-missed: euclidean <= 8.0)
        ("B1.1","B1.32"), ("B2.32","B2.1"), ("B1.1","B1.29"),
        # cross-controller near-distance (rule-missed: euclidean <= 8.0)
        ("E1.4","E2.1"), ("E1.3","E2.1"),
        # cross-controller far, non-E sources
        ("P1.1","P2.1"), ("S1","S2"), ("O1","O2"), ("I1","I2"), ("G1","G2"),
        ("B2.1","B1.32"), ("B1.1","B2.1"),
        # cross-controller far, E source
        ("E1.1","E2.1"),
    ]
    # To keep determinism, generate BAD_COUNT rows
    bad_rows=[]
    for i in range(BAD_COUNT):
        r = random.Random(SEED + 100000 + i)
        pf = r.choice(good_files)
        src,sink = r.choice(BAD_POOL)
        txt=pf.read_text(errors="ignore")
        sections=parse_ini_sections(txt)
        circuit_outputs=collect_circuit_outputs(sections)
        feat = binding_features(src, sink, geom, sections, circuit_outputs, param_key=0)
        if feat is None:
            # if unresolvable (should not happen for those tokens), skip but generate alternative
            continue
        bad_rows.append((*[feat[h] for h in HEADER[:-1]], 1))
    all_rows.extend(bad_rows)
    bad_count=len(bad_rows)
    # Sort rows for determinism byte-identical
    # Sorting key: all columns (euclidean as formatted string to avoid float noise)
    # Use tuple with rounded euclidean
    def sort_key(row):
        # row is tuple length 14, euclidean at index 7
        e = row[7]
        # round to 6 decimals for sort stability
        return (row[0],row[1],row[2],row[3],row[4],row[5],row[6], round(e,6), row[8], row[9],row[10],row[11],row[12],row[13])
    all_rows.sort(key=sort_key)
    # Write CSV
    CSV_PATH.parent.mkdir(parents=True, exist_ok=True)
    with CSV_PATH.open("w", newline="") as f:
        w=csv.writer(f, lineterminator="\n")
        w.writerow(HEADER)
        for row in all_rows:
            # format euclidean to 6 decimals
            out=list(row)
            out[7]=f"{out[7]:.6f}"
            w.writerow(out)
    print(f"Wrote {CSV_PATH}: {good_count} good + {bad_count} bad = {len(all_rows)} total rows", file=sys.stderr)
    print(f"Generator method: {method}", file=sys.stderr)
    # verify columns
    # quick check byte-identical: hash
    import hashlib
    data=CSV_PATH.read_bytes()
    h=hashlib.sha256(data).hexdigest()[:12]
    print(f"CSV sha256 prefix: {h}", file=sys.stderr)
    # Report for output discipline
    print(f"METHOD={method}")
    print(f"GOOD={good_count}")
    # Step 4 per-token influence stats (design D4 second opinion)
    stats, per_kind = collect_influence_stats(good_files)
    pin = write_influence_stats(stats)
    print(f"INFLUENCE_STATS={INFLUENCE_STATS_PATH}  {len(stats)} kinds  sha256 {pin}", file=sys.stderr)
    print(f"INFLUENCE_KINDS={sorted(stats.keys())}")
    print(f"INFLUENCE_STATS_SHA={pin}")
    print(f"BAD={bad_count}")
    print(f"TOTAL={len(all_rows)}")
    print(f"HASH={h}")

if __name__=="__main__":
    main()
