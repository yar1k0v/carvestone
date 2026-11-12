#!/usr/bin/env bash
# Runs the full checks in one call, each step through scripts/check.sh, and stops at the first failure.
# Usage: scripts/full-check.sh [--web] [--dry-run]
# The web steps run with --web, or when web/ has uncommitted changes or differs from origin/main.
# --dry-run prints each command it would run and runs none.
# It exits with the exit code of the failing step, or 2 on bad arguments.
set -u

usage="usage: scripts/full-check.sh [--web] [--dry-run]"
web=0
dry=0
for arg in "$@"; do
  case "$arg" in
    --web) web=1 ;;
    --dry-run) dry=1 ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [ "$web" -eq 0 ]; then
  if [ -n "$(git status --porcelain -- web 2>/dev/null)" ] \
    || [ -n "$(git diff --name-only origin/main...HEAD -- web 2>/dev/null)" ]; then
    web=1
  fi
fi

# run <name> <command...>: one step, named full-<name> in docs/agent-scratch.
run() {
  name="$1"
  shift
  if [ "$dry" -eq 1 ]; then
    echo "full-$name: $*"
    return 0
  fi
  bash scripts/check.sh "full-$name" "$@"
  code=$?
  if [ "$code" -ne 0 ]; then
    echo "full checks: failed at $name"
    exit "$code"
  fi
}

run fmt cargo fmt --all --check
run clippy cargo clippy --workspace --all-targets -- -D warnings
run test cargo test --workspace
run build cargo build --bin stone
run stone target/debug/stone check

if [ "$web" -eq 1 ]; then
  run web-typecheck pnpm --dir web typecheck
  run web-lint pnpm --dir web lint
  run web-test pnpm --dir web test
  run web-build pnpm --dir web build
fi

if [ "$dry" -eq 0 ]; then
  echo "full checks: passed"
fi
