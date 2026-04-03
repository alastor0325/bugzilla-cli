# Dev Rules

## Before every commit

1. **Extract logic into pure functions.** Never leave meaningful logic inline inside command handlers (`cmd_*`). Pure functions have no I/O and are directly testable.

2. **Write unit tests.** Every pure function must have tests covering its branches. If behavior is removed, add a test asserting the removal holds (e.g. no truncation).

3. **Update README.md** if a command or its flags changed.

Run `cargo test --lib` and confirm green before committing.
