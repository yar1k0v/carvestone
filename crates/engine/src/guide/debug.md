# Debugging how an agent uses stone

Walk one real task through stone with the owner, to find where tokens and turns go. The subject is your decision making, not the task. Change no code and no items.

1. Run `stone task next`. Pick one ready task that crosses several parts of the code, and say why.
2. Find out what the task needs: the related docs, the C4 architecture and where to start work. Use stone only.
3. Run one stone call per step. Never batch calls in this session.
4. After each call, print the step card below, then stop. Run the next call only when the owner says so. The owner may change it.
5. Show the real output in the card, cut down to what matters. Count its bytes.
6. Say what the output did not give you, and what you guessed. A guess is a finding.
7. Stop when you can name the first file and symbol to change, or when stone cannot take you further.

```text
STEP n ─ <the stone call>                              ~<bytes> B
─────────────────────────────────────────────────────────────
WHY     why this call, and who chose it
OUTPUT  the output, simplified
KNOW    what you learned
MISSING what you needed and did not get
DECIDE  what you concluded, and on what evidence
─────────────────────────────────────────────────────────────
NEXT ▶  the next stone call
        what you expect from it, and its size
```

End the session with:

- Totals: turns, stone calls, bytes read, and bytes that mattered.
- The path: how you decided, one line per step, and where the start point came from.
- Certain findings: seen in this run, each with its evidence.
- Likely findings: untested, each with a before-and-after test on another task. Compare turns and bytes.
