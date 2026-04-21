#!/usr/bin/env bash
# Regression test for scripts/mutation-score.py + scripts/mutation-summary.py.
#
# Guards against HEALTH-LOG §15.2's failure mode: cargo-mutants v27 renamed
# `Caught`/`Missed` → `CaughtMutant`/`MissedMutant` in outcomes.json and
# wrapped scenarios as `{"Mutant": {...}}`. The scripts were never updated
# and silently reported 0/0 for every target until the first scheduled run
# exposed it.
#
# Freezes the scripts' expected output against a hand-crafted
# `mutants-sample` fixture; any future schema drift in cargo-mutants that
# changes field names or status values will cause this test to fail fast in
# ci-lint instead of silently in the weekly mutation cron.
#
# Run locally: bash scripts/test_mutation_scripts.sh
# Run in CI:   see .github/workflows/ci-lint.yml `mutation-scripts` job

set -euo pipefail

cd "$(dirname "$0")/.."

fixture="scripts/fixtures/mutants-sample"
score_expected="scripts/fixtures/mutants-score-expected.txt"
summary_expected="scripts/fixtures/mutants-summary-expected.md"

score_actual=$(mktemp)
summary_actual=$(mktemp)
trap 'rm -f "$score_actual" "$summary_actual"' EXIT

python3 scripts/mutation-score.py --target sample --threshold 0 "$fixture/sample" > "$score_actual"
if ! diff -u "$score_expected" "$score_actual"; then
  echo ""
  echo "FAIL: scripts/mutation-score.py stdout drifted from $score_expected"
  echo "      Regenerate with:"
  echo "      python3 scripts/mutation-score.py --target sample --threshold 0 $fixture/sample > $score_expected"
  echo "      Then inspect the diff and confirm it reflects intentional parser changes"
  echo "      (not an unintended cargo-mutants schema drift) before committing."
  exit 1
fi

python3 scripts/mutation-summary.py "$fixture" > "$summary_actual"
if ! diff -u "$summary_expected" "$summary_actual"; then
  echo ""
  echo "FAIL: scripts/mutation-summary.py stdout drifted from $summary_expected"
  echo "      Regenerate with:"
  echo "      python3 scripts/mutation-summary.py $fixture > $summary_expected"
  echo "      Then inspect the diff and confirm it reflects intentional parser changes"
  echo "      (not an unintended cargo-mutants schema drift) before committing."
  exit 1
fi

echo "OK: mutation-score.py + mutation-summary.py stdout matches frozen fixtures"
