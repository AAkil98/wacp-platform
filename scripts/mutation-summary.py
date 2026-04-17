#!/usr/bin/env python3
"""Aggregate per-target cargo-mutants outputs into one Markdown summary.

Walks the directory passed on the command line (the
`actions/download-artifact` destination, one subdirectory per uploaded
artifact). Produces a single Markdown table + per-target survivor lists
suitable for piping into `$GITHUB_STEP_SUMMARY`.

Spec: wacp-console/specs/coding/wcon-mutation-testing.md §6
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def collect(target_dir: Path) -> tuple[dict[str, int], list[str]]:
    """Return (counts, survivors) for one target's outcomes.json."""
    path = target_dir / "outcomes.json"
    if not path.exists():
        return {}, []
    with path.open() as f:
        data = json.load(f)
    counts: dict[str, int] = {}
    survs: list[str] = []
    for outcome in data.get("outcomes", []):
        status = outcome.get("summary") or outcome.get("outcome") or "Unknown"
        counts[status] = counts.get(status, 0) + 1
        if status == "Missed":
            scenario = outcome.get("scenario") or {}
            mutant = scenario.get("mutant") or {}
            file = mutant.get("file") or scenario.get("file") or "<file?>"
            line = mutant.get("line") or scenario.get("line") or "?"
            mut = mutant.get("genre") or mutant.get("replacement") or scenario.get("name") or "<mutation?>"
            survs.append(f"{file}:{line} — {mut}")
    return counts, survs


def score_pct(counts: dict[str, int]) -> tuple[float, int, int]:
    """Same formula as scripts/mutation-score.py."""
    caught = counts.get("Caught", 0)
    missed = counts.get("Missed", 0)
    timeout = counts.get("Timeout", 0)
    unviable = counts.get("Unviable", 0)
    failed = counts.get("Failed", 0)
    total = caught + missed + timeout + unviable + failed
    killed = caught + timeout
    testable = total - unviable
    if testable <= 0:
        return 0.0, killed, testable
    return killed / testable * 100.0, killed, testable


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: mutation-summary.py <artifact-root>", file=sys.stderr)
        return 2
    root = Path(argv[0])
    if not root.is_dir():
        print(f"::warning::artifact root {root} does not exist")
        return 0  # Don't fail the aggregator if there's nothing to summarise.

    targets = sorted(p for p in root.iterdir() if p.is_dir())

    print("## Mutation testing")
    print()
    if not targets:
        print("_No artifacts were collected. Per-target jobs likely failed in the build phase._")
        return 0

    print("| Target | Score | Killed | Missed | Timeout | Unviable | Failed | Total |")
    print("|---|---|---|---|---|---|---|---|")
    survivor_blocks: list[tuple[str, list[str]]] = []
    for t in targets:
        counts, survs = collect(t)
        if not counts:
            print(f"| `{t.name}` | n/a ⚠ | – | – | – | – | – | – |")
            continue
        pct, killed, testable = score_pct(counts)
        marker = "✅" if pct >= 85.0 else "⚠"
        total = sum(counts.values())
        print(
            f"| `{t.name}` | {pct:.1f} % {marker} "
            f"| {killed} | {counts.get('Missed', 0)} | {counts.get('Timeout', 0)} "
            f"| {counts.get('Unviable', 0)} | {counts.get('Failed', 0)} | {total} |"
        )
        if survs:
            survivor_blocks.append((t.name, survs))

    print()
    if not survivor_blocks:
        print("_No surviving mutants across any target._")
        return 0

    print("### Surviving mutants")
    print()
    for name, survs in survivor_blocks:
        print(f"#### `{name}` ({len(survs)})")
        print()
        for s in survs[:50]:
            print(f"- `{s}`")
        if len(survs) > 50:
            print(f"- … and {len(survs) - 50} more (see uploaded artifact)")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
