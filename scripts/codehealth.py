#!/usr/bin/env python3
"""Code-health gate for droid_tui: complexity, duplication, dead-code policy.

Three checks over `src/` production code (test modules are excluded):

1. Complexity: decision points per function. A decision point is one of
   `if`, `for`, `while`, `loop`, a `match` arm (`=>`), `&&`, `||`, `?`,
   or the `unwrap_or`/`map_or`/`and_then`/`or_else` combinators.
   Budget: `COMPLEXITY_BUDGET` per function.
2. Duplication: identical runs of `DUPLICATE_WINDOW` normalised lines
   (comments and blank lines removed) appearing at two or more distinct
   places. Budget: none.
3. Dead-code policy: no `#[allow(dead_code)]`/`#[expect(dead_code)]` in
   production code (`cargo clippy -D warnings` already denies the lint).

Usage:
    python3 scripts/codehealth.py                  # gate, exit 1 on a violation
    python3 scripts/codehealth.py --update-baseline # record today's offenders

`scripts/codehealth-baseline.txt` records the pre-existing offenders so the
gate fails on NEW debt only: an entry present in the baseline and unchanged
does not fail, anything absent from it or grown does. Deleting an offender
from the code means deleting its line from the baseline in the same change.
"""

import hashlib
import re
import sys
from collections import defaultdict
from pathlib import Path

COMPLEXITY_BUDGET = 15
DUPLICATE_WINDOW = 8

FN_START = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?fn\s+([A-Za-z0-9_]+)")
DECISION = re.compile(r"\b(?:if|for|while|loop)\b|=>|&&|\|\||\?|\b(?:unwrap_or|map_or|and_then|or_else)\b")
STR_LIT = re.compile(r'"(?:\\.|[^"\\])*"')
CHAR_LIT = re.compile(r"'(?:\\.|[^'\\])'")
LINE_COMMENT = re.compile(r"//.*$")
WHITESPACE = re.compile(r"\s+")
DEAD_CODE_ALLOW = re.compile(r"#\[(?:allow|expect)\(dead_code\)\]")


def repo_root() -> Path:
    for base in [Path(__file__).resolve().parent, *Path(__file__).resolve().parents]:
        if (base / "Cargo.toml").is_file() and (base / "src").is_dir():
            return base
    raise SystemExit("codehealth: no Cargo.toml + src/ above this script")


ROOT = repo_root()
SRC = ROOT / "src"
BASELINE = ROOT / "scripts" / "codehealth-baseline.txt"


def strip_noise(line: str) -> str:
    line = LINE_COMMENT.sub("", line)
    line = STR_LIT.sub('""', line)
    return CHAR_LIT.sub("''", line)


def production_lines(lines):
    """Yield (line_number, code) outside `#[cfg(test)]` modules and `#[test]` fns."""
    skip_depth = 0          # >0 while inside a skipped block
    depth = 0
    pending_cfg_test = False
    for number, raw in enumerate(lines, start=1):
        stripped = raw.strip()
        if skip_depth == 0 and stripped == "#[cfg(test)]":
            pending_cfg_test = True
            continue
        code = strip_noise(raw)
        if skip_depth > 0:
            skip_depth += code.count("{") - code.count("}")
            if skip_depth <= 0:
                skip_depth = 0
            continue
        if pending_cfg_test:
            pending_cfg_test = False
            if "{" in code:
                opened = code.count("{") - code.count("}")
                if opened > 0:
                    skip_depth = opened
                    continue
        if stripped == "#[test]":
            pending_cfg_test = True
        depth += code.count("{") - code.count("}")
        yield number, code


def functions(path: Path):
    """Yield (name, line_number, decision_points) for production `fn`s."""
    lines = path.read_text(encoding="utf-8").splitlines()
    numbered = list(production_lines(lines))
    index = 0
    while index < len(numbered):
        number, code = numbered[index]
        match = FN_START.match(code)
        if not match:
            index += 1
            continue
        name = match.group(1)
        start = number
        opened = "{" in code
        depth = code.count("{") - code.count("}")
        body = [code]
        index += 1
        # A rustfmt-wrapped signature puts the `{` on a later line, so keep
        # consuming until the body actually opens (or a body-less trait method
        # ends in `;` at depth 0).
        while index < len(numbered):
            if opened and depth <= 0:
                break
            _, more = numbered[index]
            if not opened and ";" in more and depth <= 0:
                index += 1
                break
            if "{" in more:
                opened = True
            depth += more.count("{") - more.count("}")
            body.append(more)
            index += 1
        yield name, start, len(DECISION.findall("\n".join(body)))


def normalised_windows(path: Path):
    """Yield (window_hash, first_line, window_text) for the file's production lines."""
    lines = path.read_text(encoding="utf-8").splitlines()
    kept = []
    for number, code in production_lines(lines):
        text = WHITESPACE.sub(" ", code.strip())
        if not text or text.startswith(("use ", "pub use ", "mod ", "#[", "#![")):
            continue
        kept.append((number, text))
    for start in range(len(kept) - DUPLICATE_WINDOW + 1):
        chunk = kept[start : start + DUPLICATE_WINDOW]
        text = "\n".join(part for _, part in chunk)
        if sum(len(part) for _, part in chunk) < 240:
            continue
        digest = hashlib.sha1(text.encode("utf-8")).hexdigest()[:12]
        yield digest, chunk[0][0], text


def collect():
    complexity = []
    totals = 0
    files = sorted(SRC.rglob("*.rs"))
    for path in files:
        rel = path.relative_to(ROOT).as_posix()
        for name, line, points in functions(path):
            totals += 1
            if points > COMPLEXITY_BUDGET:
                complexity.append((rel, name, points))
    duplicates = defaultdict(list)
    for path in files:
        rel = path.relative_to(ROOT).as_posix()
        for digest, line, _ in normalised_windows(path):
            duplicates[digest].append(f"{rel}:{line}")
    duplicated = {digest: sorted(where) for digest, where in duplicates.items() if len(where) > 1}
    allowances = []
    for path in files:
        rel = path.relative_to(ROOT).as_posix()
        for number, code in production_lines(path.read_text(encoding="utf-8").splitlines()):
            if DEAD_CODE_ALLOW.search(code):
                allowances.append(f"{rel}:{number}")
    return totals, complexity, duplicated, allowances


def read_baseline():
    known_complexity = {}
    known_duplicates = set()
    known_allow = set()
    if not BASELINE.is_file():
        return known_complexity, known_duplicates, known_allow
    for raw in BASELINE.read_text(encoding="utf-8").splitlines():
        parts = raw.split()
        if not parts or parts[0].startswith("#"):
            continue
        if parts[0] == "complexity" and len(parts) == 4:
            known_complexity[(parts[1], parts[2])] = int(parts[3])
        elif parts[0] == "duplicate" and len(parts) == 2:
            known_duplicates.add(parts[1])
        elif parts[0] == "dead-code" and len(parts) == 2:
            known_allow.add(parts[1])
    return known_complexity, known_duplicates, known_allow


def write_baseline(totals, complexity, duplicated, allowances):
    lines = [
        "# Code-health baseline: pre-existing offenders recorded by",
        "# `python3 scripts/codehealth.py --update-baseline`.",
        "# Remove a line when the offender is fixed; never add one by hand.",
        f"# budget: {COMPLEXITY_BUDGET} decision points per function; "
        f"duplicate window {DUPLICATE_WINDOW} lines",
        "",
    ]
    for rel, name, points in sorted(complexity, key=lambda item: (-item[2], item[0], item[1])):
        lines.append(f"complexity {rel} {name} {points}")
    for digest, where in sorted(duplicated.items()):
        lines.append(f"duplicate {digest}")
        lines.append(f"  # {', '.join(where)}")
    for where in allowances:
        lines.append(f"dead-code {where}")
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"baseline written: {len(complexity)} complex, {len(duplicated)} duplicated, "
          f"{len(allowances)} dead-code allowance(s) over {totals} functions")


def main(argv):
    totals, complexity, duplicated, allowances = collect()
    if "--update-baseline" in argv:
        write_baseline(totals, complexity, duplicated, allowances)
        return 0

    known_complexity, known_duplicates, known_allow = read_baseline()
    failed = False

    print(f"complexity: {totals} functions, budget {COMPLEXITY_BUDGET} decision points")
    new_complexity = []
    for rel, name, points in complexity:
        limit = known_complexity.get((rel, name))
        if limit is None or points > limit:
            new_complexity.append((points, rel, name, limit))
    for points, rel, name, limit in sorted(new_complexity, reverse=True):
        failed = True
        reason = "new" if limit is None else f"grew from {limit}"
        print(f"  {points:3d}  {rel}  fn {name}  ({reason})")
    if not new_complexity:
        print(f"  no new debt ({len(complexity)} recorded offenders, budget {COMPLEXITY_BUDGET})")

    print(f"duplication: {len(duplicated)} duplicated {DUPLICATE_WINDOW}-line block(s)")
    new_duplicates = [digest for digest in duplicated if digest not in known_duplicates]
    for digest in sorted(new_duplicates):
        failed = True
        print(f"  new  {', '.join(duplicated[digest])}")
    if not new_duplicates:
        print(f"  no new debt ({len(known_duplicates)} recorded block(s))")

    new_allow = [where for where in allowances if where not in known_allow]
    print(f"dead-code allowances: {len(allowances)}")
    for where in new_allow:
        failed = True
        print(f"  new  {where}")
    if not new_allow:
        print("  no new `#[allow(dead_code)]`")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
