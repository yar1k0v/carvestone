#!/usr/bin/env bash
# Runs a command, keeps its full output in docs/agent-scratch/<label>.txt and prints only the result lines.
# Usage: scripts/check.sh <label> <command...>
# It prints the `test result:` lines, on failure the first problem lines, then `exit <code>` and the log path.
# It exits with the exit code of the command, or 2 on bad arguments.
set -u

usage="usage: scripts/check.sh <label> <command...>"
if [ "$#" -lt 2 ]; then
  echo "$usage" >&2
  exit 2
fi

label="$1"
case "$label" in
  # The first character is a letter or digit, so the label is never empty, `.` or `..`.
  ""|[!A-Za-z0-9]*|*[!A-Za-z0-9._-]*) echo "$usage" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

mkdir -p docs/agent-scratch
log="docs/agent-scratch/$label.txt"
shift

"$@" >"$log" 2>&1
code=$?

# Windows tools may end lines with CR, so every filter below drops it first.
awk '{ sub(/\r$/, "") } /^ *test result:/ { sub(/^ */, ""); print }' "$log"

if [ "$code" -ne 0 ]; then
  # Each hit comes with the 6 lines after it: they hold the `-->` location of a compile
  # error or the assertion values of a panic. Result lines were printed above, so they are
  # neither hits nor part of a block. Overlapping blocks print each line once.
  problems="$(awk '
    { sub(/\r$/, "") }
    /^ *test result:/ { next }
    /error|panicked|FAILED/ { ctx = 7 }
    ctx > 0 { ctx--; if (shown < 40) { print; shown++ } }
  ' "$log")"
  if [ -n "$problems" ]; then
    echo "$problems"
  else
    tail -n 15 "$log"
  fi
else
  # A filter that matches no test still passes, so a zero total is flagged.
  awk '
    /^ *test result:/ {
      lines++
      if (match($0, /[0-9]+ passed/)) passed += substr($0, RSTART, RLENGTH - 7)
    }
    END { if (lines > 0 && passed == 0) print "warning: no test ran" }
  ' "$log"
fi

echo "exit $code"
echo "log: $log"
exit "$code"
