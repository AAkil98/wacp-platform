#!/usr/bin/env bash
# Install the opt-in hooks from .githooks/ into this clone.
#
# Sets core.hooksPath at repo scope only — no global git config is
# touched. Run once per clone; run again after `.githooks/` changes
# only if you want to confirm what's active.
#
# Uninstall:  git config --unset core.hooksPath

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

git config core.hooksPath .githooks

echo "installed hooks from .githooks/"
echo
echo "active hooks:"
for hook in .githooks/*; do
  if [[ -f "$hook" && -x "$hook" ]]; then
    echo "  $(basename "$hook")"
  fi
done
echo
echo "to bypass once:   git push --no-verify"
echo "to uninstall:     git config --unset core.hooksPath"
