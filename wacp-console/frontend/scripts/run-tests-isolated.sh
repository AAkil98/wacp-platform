#!/usr/bin/env bash
# Run each vitest file in its own `vitest run <file>` invocation.
#
# Each file gets a fresh Node process, so module heap, jsdom globals,
# and React tree refs cannot accumulate across files. Trades ~1-3s
# startup cost per file for a hard guarantee that no leak can climb
# past one file's worth of allocations.
#
# Usage:
#   ./scripts/run-tests-isolated.sh               # all *.test.{ts,tsx} under src/
#   ./scripts/run-tests-isolated.sh src/foo.test.tsx src/bar.test.tsx
#
# Env:
#   VITEST_ARGS    extra args forwarded to each `vitest run` invocation
#   HEAP_MB        value for --max-old-space-size on each worker (default 1536)

set -u
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$FRONTEND_DIR"

HEAP_MB="${HEAP_MB:-1536}"
VITEST_ARGS="${VITEST_ARGS:-}"

if [[ $# -gt 0 ]]; then
  FILES=("$@")
else
  mapfile -t FILES < <(find src -type f \( -name '*.test.ts' -o -name '*.test.tsx' \) | sort)
fi

total=${#FILES[@]}
if [[ $total -eq 0 ]]; then
  echo "No test files found."
  exit 0
fi

echo "Running $total test files in isolated processes (heap limit ${HEAP_MB}MB per process)"
echo

declare -a FAILED=()
start_all=$(date +%s)

i=0
for file in "${FILES[@]}"; do
  i=$((i + 1))
  echo "[$i/$total] $file"
  file_start=$(date +%s)

  # NODE_OPTIONS applies to the vitest CLI and any children it forks.
  if NODE_OPTIONS="--max-old-space-size=${HEAP_MB}" \
     npx vitest run "$file" $VITEST_ARGS; then
    file_end=$(date +%s)
    echo "    pass ($((file_end - file_start))s)"
  else
    file_end=$(date +%s)
    echo "    FAIL ($((file_end - file_start))s)"
    FAILED+=("$file")
  fi
  echo
done

end_all=$(date +%s)
walltime=$((end_all - start_all))

echo "================================================================"
echo "Ran $total files in ${walltime}s"
if [[ ${#FAILED[@]} -eq 0 ]]; then
  echo "All files passed."
  exit 0
else
  echo "${#FAILED[@]} file(s) failed:"
  for f in "${FAILED[@]}"; do
    echo "  - $f"
  done
  exit 1
fi
