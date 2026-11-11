#!/usr/bin/env bash
# Builds the release binary with the UI into build/. An optional argument bumps the version first.
# Usage: scripts/build.sh [patch|minor|major]
# It never commits, tags or pushes: a bump leaves Cargo.toml and Cargo.lock changed.
set -euo pipefail

usage="usage: scripts/build.sh [patch|minor|major]"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

part="${1:-}"
case "$part" in
  ""|patch|minor|major) ;;
  *) echo "$usage" >&2; exit 1 ;;
esac
if [ "$#" -gt 1 ]; then
  echo "$usage" >&2
  exit 1
fi

if [ -n "$part" ]; then
  # The first `version = "X.Y.Z"` line is the one under [workspace.package].
  line="$(grep -n -m1 -E '^version = "[0-9]+\.[0-9]+\.[0-9]+"$' Cargo.toml || true)"
  if [ -z "$line" ]; then
    echo "Cargo.toml has no [workspace.package] version line of the form version = \"X.Y.Z\"." >&2
    exit 1
  fi
  lineno="${line%%:*}"
  old="$(echo "$line" | sed -E 's/^[0-9]+:version = "([^"]+)"$/\1/')"
  IFS=. read -r major minor patch <<< "$old"
  case "$part" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
  esac
  new="$major.$minor.$patch"
  sed -i "${lineno}s/.*/version = \"$new\"/" Cargo.toml
  echo "version $old -> $new"
fi

pnpm --dir web build
cargo build --release --features ui --bin stone --target-dir target/build-release

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) bin=stone.exe ;;
  *) bin=stone ;;
esac
mkdir -p build
cp "target/build-release/release/$bin" "build/$bin"
"build/$bin" --version
