# Testing rules

A test answers one question: does this feature work as expected? There is no target count and no coverage target.

Valuable logic: behavior a user depends on, saved formats, lock and write rules, parsing and resolving, and decisions with real branches.
Not valuable: wiring, pass-through code, private helpers already reached by a public path, and facts the compiler enforces.

## Types first

- Closed set of values: use an enum or newtype, and let compiler errors lead the change. Write no test for what the compiler enforces.
- Outside data (config, documents, user input) stays data. Test the constructor or parser that reads it.

## Choose

1. Name the invariant, and the defect that would break it. Include platform differences. No defect to name: no test.
2. Find the closest existing test of that behavior.
3. Choose one, and state it in one line:
   - Patch (default): add a row, fixture or assertion to that test.
   - Add: new public behavior, a saved format or lock rule, or a bug no existing test can reach. Say why patching wasn't enough.
   - None: the logic isn't valuable, it is a refactor with green tests, or it is docs, config or styling.

## Loop

1. Red: the new case fails because the behavior is missing. For a missing symbol, add the signature with a stub, so it fails on the assertion. A compile error doesn't count.
2. Green: add the least code that passes.
3. Clean up while the tests stay green.

Rework of existing tests: the code already exists, so a new test can't fail first. Prove it instead: break the code by hand once per scenario, and the test must fail.

## Write

- Test through the public API. Test private code only when it holds real logic.
- Test the parser, not the process call. Smoke tests cover CLI wiring.
- Expected values come from an independent source: a known literal, a worked example or the spec. Never recompute them the way the code does.
- Never weaken a valid test to get green. If the behavior changed on purpose, say so in the task.
- All wrong input and error cases of a feature go in one table test, never one test each. Other similar cases go in one table or fixture file.
- Use a property test for round-trips, idempotence and "never panics".
- Prefer pure inputs: no sleeps and no new git repos. Reuse the shared fixtures.
- Delete a test that does not check a feature. It needs no named replacement.
