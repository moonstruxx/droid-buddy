#!/usr/bin/env python3
"""Optional, offline-tolerant advisory sync for the dependency risk evaluation.

Queries the services that answer without credentials first:

  1. OSV (https://api.osv.dev)              - the RustSec advisory data, per crate+version
  2. cve.org  (https://cveawg.mitre.org)    - the authoritative CVE record, CVSS and state
  3. OpenCVE  (https://app.opencve.io)      - only when OPENCVE_API_KEY is set; it is 401 without one

Nothing here is part of a build, a test or a CI gate: SECURITY.md records a dated
evaluation, and this script re-measures it on request. When the network is unavailable the
script says so and leaves the recorded evaluation in place - it never fails a build and
never blocks a commit.

Usage:
    python3 scripts/security-sync.py                # scan both manifests, print the report
    python3 scripts/security-sync.py --json out.json
    python3 scripts/security-sync.py --strict       # exit 1 on a finding, 3 on a failed query
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request

OSV_BATCH = "https://api.osv.dev/v1/querybatch"
CVEORG = "https://cveawg.mitre.org/api/cve/{}"
OPENCVE = "https://app.opencve.io/api/cve/{}"
TIMEOUT = 20

MANIFESTS = [
    ("app", ["tree"]),
    ("tools/screenshots", ["tree", "--manifest-path", "tools/screenshots/Cargo.toml"]),
]


class Offline(Exception):
    """No answer from a service: the recorded evaluation stands."""


def post_json(url: str, payload: dict) -> dict:
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as response:
            return json.loads(response.read())
    except (urllib.error.URLError, TimeoutError, OSError) as error:
        raise Offline(f"{url}: {error}") from error


def get_json(url: str, headers: dict | None = None) -> dict:
    req = urllib.request.Request(url, headers=headers or {})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as response:
            return json.loads(response.read())
    except (urllib.error.URLError, TimeoutError, OSError) as error:
        raise Offline(f"{url}: {error}") from error


def resolved_packages(command: list[str]) -> list[tuple[str, str]]:
    """Every package the manifest resolves, as (name, version) without build metadata."""
    try:
        out = subprocess.run(
            ["cargo", *command, "--prefix", "none", "--format", "{p}"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        raise Offline(f"cargo tree failed: {error}") from error
    packages = set()
    for line in out.splitlines():
        line = line.replace(" (*)", "").strip()
        match = re.match(r"^([A-Za-z0-9_-]+) v([0-9][^\s]*)", line)
        if match:
            packages.add((match.group(1), match.group(2).split("+")[0]))
    return sorted(packages)


def cve_records(advisory: dict) -> list[dict]:
    """The cve.org record for each CVE / GHSA alias the advisory names."""
    records = []
    for alias in advisory.get("aliases", []):
        if not (alias.startswith("CVE-") or alias.startswith("GHSA-")):
            continue
        record = get_json(CVEORG.format(alias))
        container = record.get("containers", {}).get("cna", {})
        metrics = container.get("metrics", [])
        score = next(
            (m.get("cvssV4_0", {}).get("baseScore") or m.get("cvssV3_1", {}).get("baseScore")
             for m in metrics if m.get("cvssV4_0") or m.get("cvssV3_1")),
            None,
        )
        records.append(
            {
                "alias": alias,
                "state": record.get("cveMetadata", {}).get("state"),
                "cvss": score,
                "title": container.get("title"),
            }
        )
    return records


def opencve_view(alias: str, key: str) -> dict | None:
    try:
        return get_json(OPENCVE.format(alias), {"Authorization": f"Bearer {key}"})
    except Offline:
        return None


def scan(manifest: str, command: list[str], opencve_key: str | None) -> list[dict]:
    packages = resolved_packages(command)
    payload = {
        "queries": [
            {"package": {"name": name, "ecosystem": "crates.io"}, "version": version}
            for name, version in packages
        ]
    }
    answer = post_json(OSV_BATCH, payload)
    findings = []
    for (name, version), result in zip(packages, answer.get("results", [])):
        for advisory in result.get("vulns", []) or []:
            detail = get_json(f"https://api.osv.dev/v1/vulns/{advisory['id']}")
            entry = {
                "manifest": manifest,
                "crate": name,
                "version": version,
                "advisory": advisory["id"],
                "aliases": advisory.get("aliases", []),
                "summary": (advisory.get("summary") or detail.get("summary") or "").strip(),
                "cves": cve_records(advisory),
                "fixed": sorted(
                    {
                        event["fixed"]
                        for affected in detail.get("affected", [])
                        for range_ in affected.get("ranges", [])
                        for event in range_.get("events", [])
                        if "fixed" in event
                    }
                ),
            }
            if opencve_key:
                alias = next(
                    (a for a in advisory.get("aliases", []) if a.startswith("CVE-")), None
                )
                if alias:
                    entry["opencve"] = bool(opencve_view(alias, opencve_key))
            findings.append(entry)
    return findings


def main(argv: list[str]) -> int:
    strict = "--strict" in argv
    json_path = None
    if "--json" in argv:
        json_path = argv[argv.index("--json") + 1]

    opencve_key = os.environ.get("OPENCVE_API_KEY")
    report = {"services": ["osv.dev", "cveawg.mitre.org"] + (["app.opencve.io"] if opencve_key else []),
              "manifests": {}, "findings": []}
    offline: list[str] = []

    for manifest, command in MANIFESTS:
        try:
            packages = resolved_packages(command)
            report["manifests"][manifest] = len(packages)
            report["findings"].extend(scan(manifest, command, opencve_key))
        except Offline as error:
            offline.append(str(error))

    merged: dict[tuple[str, str, str], dict] = {}
    for finding in report["findings"]:
        key = (finding["advisory"], finding["crate"], finding["version"])
        if key in merged:
            merged[key]["manifests"].append(finding["manifest"])
        else:
            finding["manifests"] = [finding.pop("manifest")]
            merged[key] = finding
    report["findings"] = sorted(merged.values(), key=lambda f: (f["advisory"], f["crate"]))

    if offline:
        print("offline: the recorded evaluation in SECURITY.md stands unchanged.")
        for line in offline:
            print(f"  - {line}")
        if strict:
            return 3
        return 0

    print(f"scanned {sum(report['manifests'].values())} resolved packages "
          f"({', '.join(f'{m}={n}' for m, n in report['manifests'].items())})")
    if not report["findings"]:
        print("no advisory affects a resolved version")
        return 0

    for finding in report["findings"]:
        cves = ", ".join(c["alias"] for c in finding["cves"]) or "no CVE alias"
        cvss = next((c["cvss"] for c in finding["cves"] if c["cvss"]), None)
        print(f"  {finding['advisory']} ({cves}{f', CVSS {cvss}' if cvss else ''}) "
              f"{finding['crate']} {finding['version']} "
              f"-> fixed {', '.join(finding['fixed']) or 'no fixed version'}")
        print(f"      {finding['summary']}")

    if json_path:
        with open(json_path, "w", encoding="utf-8") as handle:
            json.dump(report, handle, indent=1)
        print(f"report written to {json_path}")

    return 1 if strict else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
