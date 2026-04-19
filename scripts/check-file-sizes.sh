#!/usr/bin/env bash
# File-size guardrail for the wacp-platform repo.
#
# Walks tracked Rust + TypeScript files, compares line counts against
# thresholds, emits GitHub-annotation-compatible warnings/errors, and
# exits 1 if any non-allowlisted file exceeds its fail threshold.
#
# Thresholds (from tech-debt-2026-04-18.md §3.3):
#   Rust  — warn > 1000 lines, fail > 1500 lines
#   TS    — warn >  500 lines, fail > 1000 lines
#
# Exemptions live in .file-size-allowlist at repo root. Shrink-only
# convention: removing entries is free; adding one requires explicit
# review. See tech-debt-2026-04-18.md §5 for the allowlist rationale.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

ALLOWLIST=".file-size-allowlist"
RUST_WARN=1000
RUST_FAIL=1500
TS_WARN=500
TS_FAIL=1000

declare -A ALLOWED
if [[ -f "$ALLOWLIST" ]]; then
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%%#*}"
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" ]] && continue
    ALLOWED["$line"]=1
  done < "$ALLOWLIST"
fi

fails=0
warns=0

check() {
  local file=$1 warn=$2 fail=$3
  file="${file#./}"
  local lines
  lines=$(wc -l < "$file")

  if [[ -n "${ALLOWED[$file]:-}" ]]; then
    return 0
  fi

  if (( lines > fail )); then
    echo "::error file=${file}::FILE-SIZE: ${lines} lines > fail threshold ${fail}. Split the file, or add it to .file-size-allowlist with a justification comment."
    fails=$((fails + 1))
  elif (( lines > warn )); then
    echo "::warning file=${file}::FILE-SIZE: ${lines} lines > warn threshold ${warn}. Consider splitting."
    warns=$((warns + 1))
  fi
}

while IFS= read -r -d '' file; do
  check "$file" "$RUST_WARN" "$RUST_FAIL"
done < <(find . -type f -name "*.rs" \
  -not -path "./target/*" \
  -not -path "*/node_modules/*" \
  -not -path "./.git/*" \
  -print0)

while IFS= read -r -d '' file; do
  check "$file" "$TS_WARN" "$TS_FAIL"
done < <(find . -type f \( -name "*.ts" -o -name "*.tsx" \) \
  -not -path "./target/*" \
  -not -path "*/node_modules/*" \
  -not -path "*/dist/*" \
  -not -path "./.git/*" \
  -print0)

echo ""
echo "File-size check: ${warns} warning(s), ${fails} failure(s)"

if (( fails > 0 )); then
  echo ""
  echo "::error::File-size check failed. Refactor the oversized file(s) above, or add them to .file-size-allowlist (top of repo) with a '# reason: ...' comment. Allowlist adds must justify shrink-only intent."
  exit 1
fi
