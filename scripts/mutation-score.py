#!/usr/bin/env python3
"""Compute the mutation score from a `cargo-mutants` output directory.

Reads `<dir>/outcomes.json`, computes:

    killed   = caught + timeout
    testable = total - unviable
    score    = killed / testable * 100

Prints a one-line summary and the survivor list to stdout. Exits 0 when
the score is >= the threshold; exits 1 otherwise (caller's failing job).
Also writes the per-target summary block to `$GITHUB_STEP_SUMMARY` when
that env var is set, so individual jobs surface their own breakdown
alongside the aggregator (run by `mutation-summary.py`).

Spec: wacp-console/specs/coding/wcon-mutation-testing.md §5
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path


# cargo-mutants v27 emits `CaughtMutant` / `MissedMutant` in the summary
# field; earlier versions used `Caught` / `Missed`. Normalize to the short
# names at parse time so the rest of the script can stay version-agnostic.
# `Success` flags the baseline run (not a mutant outcome) — skip it.
STATUS_ALIASES: dict[str, str] = {
    "CaughtMutant": "Caught",
    "MissedMutant": "Missed",
    "TimedOut": "Timeout",
}


def parse_outcomes(out_dir: Path) -> dict[str, int]:
    """Read cargo-mutants outcomes and return a per-status histogram.

    cargo-mutants writes `outcomes.json` with shape `{ "outcomes": [...] }`.
    Each outcome's status appears in the `summary` field as one of
    "Caught", "Missed", "Timeout", "Unviable", "Failed" (legacy) or
    "CaughtMutant", "MissedMutant", ... (v27+).
    """
    path = out_dir / "outcomes.json"
    if not path.exists():
        # cargo-mutants may not produce outcomes.json if the build itself
        # failed before any mutation ran. Treat that as "unmeasurable" so
        # the aggregator can flag it; do not falsely report 100 %.
        print(f"::warning::no outcomes.json in {out_dir} — build error?")
        return {}
    with path.open() as f:
        data = json.load(f)
    counts: dict[str, int] = {}
    for outcome in data.get("outcomes", []):
        raw = outcome.get("summary") or outcome.get("outcome") or "Unknown"
        status = STATUS_ALIASES.get(raw, raw)
        if status == "Success":
            # Baseline run — not a mutant outcome.
            continue
        counts[status] = counts.get(status, 0) + 1
    return counts


def score(counts: dict[str, int]) -> tuple[float, int, int]:
    """Return (score_percent, killed, testable)."""
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


def survivors(out_dir: Path) -> list[str]:
    """Return one-line descriptions of surviving (Missed) mutants."""
    path = out_dir / "outcomes.json"
    if not path.exists():
        return []
    with path.open() as f:
        data = json.load(f)
    out: list[str] = []
    for outcome in data.get("outcomes", []):
        raw = outcome.get("summary") or outcome.get("outcome")
        status = STATUS_ALIASES.get(raw, raw)
        if status != "Missed":
            continue
        # v27 wraps the mutant in {"Mutant": {...}} with line under span.start;
        # the baseline scenario is the literal string "Baseline" — skip that.
        scenario = outcome.get("scenario")
        if not isinstance(scenario, dict):
            continue
        mutant = scenario.get("Mutant") or scenario.get("mutant") or {}
        file = mutant.get("file") or "<file?>"
        span = mutant.get("span") or {}
        start = span.get("start") or {}
        line = start.get("line") or mutant.get("line") or "?"
        description = mutant.get("name") or mutant.get("replacement") or mutant.get("genre") or "<mutation?>"
        out.append(f"{file}:{line} — {description}")
    return out


def emit_step_summary(target: str, counts: dict[str, int], pct: float, threshold: float, survs: list[str]) -> None:
    """If running in GH Actions, append a per-target block to GITHUB_STEP_SUMMARY."""
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not summary_path:
        return
    status_marker = "✅" if pct >= threshold else "⚠️"
    lines = [
        f"## {status_marker} {target}: {pct:.1f} %",
        "",
        f"| Caught | Missed | Timeout | Unviable | Failed |",
        f"|---|---|---|---|---|",
        f"| {counts.get('Caught', 0)} | {counts.get('Missed', 0)} | {counts.get('Timeout', 0)} | {counts.get('Unviable', 0)} | {counts.get('Failed', 0)} |",
        "",
    ]
    if survs:
        lines.append(f"### Surviving mutants ({len(survs)})")
        lines.append("")
        for s in survs[:50]:
            lines.append(f"- `{s}`")
        if len(survs) > 50:
            lines.append(f"- … and {len(survs) - 50} more (see artifact)")
        lines.append("")
    with open(summary_path, "a") as f:
        f.write("\n".join(lines) + "\n")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, help="Human-friendly target name for the summary header")
    parser.add_argument("--threshold", type=float, default=85.0, help="Minimum acceptable mutation score (percent)")
    parser.add_argument("out_dir", type=Path, help="Path to the cargo-mutants output directory")
    args = parser.parse_args(argv)

    counts = parse_outcomes(args.out_dir)
    if not counts:
        print(f"FAIL {args.target}: no mutants ran (build error or empty target)")
        return 1
    pct, killed, testable = score(counts)
    survs = survivors(args.out_dir)
    print(
        f"{args.target}: {pct:.1f} % ({killed}/{testable}) "
        f"[Caught={counts.get('Caught', 0)} Missed={counts.get('Missed', 0)} "
        f"Timeout={counts.get('Timeout', 0)} Unviable={counts.get('Unviable', 0)} "
        f"Failed={counts.get('Failed', 0)}]"
    )
    if survs:
        print(f"Surviving mutants ({len(survs)}):")
        for s in survs[:20]:
            print(f"  {s}")
        if len(survs) > 20:
            print(f"  ... and {len(survs) - 20} more (see uploaded artifact)")

    emit_step_summary(args.target, counts, pct, args.threshold, survs)

    if pct < args.threshold:
        print(f"FAIL {args.target}: score {pct:.1f} % below threshold {args.threshold:.1f} %")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
