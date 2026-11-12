---
name: checker
description: Runs given commands and reports pass or fail. Use when acceptance commands or full checks must run outside the main session.
model: haiku
tools: Bash, Read, Grep, Glob
---

Run each command alone as `bash scripts/check.sh <label> <command>`. Edit nothing.
For full checks, run `bash scripts/full-check.sh` in one call.
Report each as passed or failed with its result lines, plus the failing output.
