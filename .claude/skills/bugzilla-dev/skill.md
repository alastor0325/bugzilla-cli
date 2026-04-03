# Bugzilla CLI Dev Loop

The **Bugzilla CLI Dev Loop** is a mandatory workflow for all code changes. All steps are required and cannot be skipped.

## The Cycle

Development proceeds through six sequential steps: understand the task, extract & develop, write tests, agent review, commit & push, then conclude.

## Core Requirements

- All new or changed logic must be extracted into **pure functions** (no `get_client()`, no I/O) so they are directly testable.
- Every pure function must have **unit tests** covering its branches. If behavior is removed, a test must assert the removal holds.
- **`cargo test --lib` must pass** before committing. Failing tests are a hard blocker — fix them, do not work around them.
- **README.md must be updated** whenever a command is added/removed or a flag/default changes.

## Process Details

### Step 1 — Understand
Read the relevant source files before touching anything. Understand the existing structure: which command handler is involved, which pure functions already exist, what tests already cover.

### Step 2 — Extract & Develop
Write the implementation. If logic belongs in a command handler (`cmd_*`), extract it into a named pure function first, then call it from the handler. Keep command handlers thin — they only wire up `get_client()`, call pure functions, and print results.

### Step 3 — Write Tests
For every pure function added or changed, write unit tests in the `#[cfg(test)] mod tests` block in `src/main.rs`:
- Happy path
- Each meaningful flag or branch (e.g. `full_text`, `all_statuses`)
- Regression guards for removed behavior

Run and confirm green:
```
cargo test --lib
```

### Step 4 — Agent Review
Run `/simplify` to have a fresh-context agent review the changes for code quality, reuse, and efficiency. Apply any fixes before committing.

### Step 5 — Commit & Push
```
git commit -m "<type>: <what and why>"
git push
```
Both are required. Never commit without pushing.

### Step 6 — Conclude
Summarize: what changed, what tests were added, whether README was updated.
