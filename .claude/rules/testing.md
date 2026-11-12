# Testing rules

A test answers one question: does this feature work as expected? Set no target count and no coverage target.

## Value

1. Test behavior a user depends on, saved formats, lock and write rules, parsing and resolving, and decisions with real branches.
2. Skip wiring, pass-through code, private helpers a public path reaches, and facts the compiler enforces.

## Types first

1. Model a closed set of values as an enum or newtype, and let compiler errors lead the change. Write no test for what the compiler enforces.
2. Keep outside data (config, documents, input) as data. Test the constructor or parser that reads it.

## Choose

1. Name the invariant and the defect that would break it, platform differences included. No defect: no test.
2. Find the closest existing test of that behavior.
3. State one choice in one line:
   - Patch (default): add a row, fixture or assertion to that test.
   - Add: new public behavior, saved format, lock rule, or a bug no test reaches. Say why patching failed.
   - None: no valuable logic, a refactor with green tests, or docs, config, styling.

## Loop

1. Red: the new case fails on the assertion because the behavior is missing. For a missing symbol, add a stub signature. A compile error does not count.
2. Green: add the least code that passes.
3. Clean up while the tests stay green.
4. For rework of existing tests, break the code by hand once per scenario. The test MUST fail.

## Write

1. Test through the public API. Test private code only when it holds real logic.
2. Test the parser, not the process call. Smoke tests cover CLI wiring.
3. Take expected values from a literal, a worked example or the spec. Never recompute them like the code.
4. NEVER weaken a valid test to get green. Say a deliberate behavior change in the task.
5. Put all wrong-input and error cases of a feature in one table test, other similar cases in one table or fixture file.
6. Use a property test for round-trips, idempotence and "never panics".
7. Use pure inputs: no sleeps, no new git repos. Reuse the shared fixtures.
8. Delete a test that checks no feature. It needs no replacement.
