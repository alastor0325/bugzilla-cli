---
description: >
  Release a new bugzilla-cli version: bump Cargo.toml, test, commit, tag, push,
  publish to crates.io, THEN file an issue on the fx-bug-toolkit repo asking it to
  bump the pinned bugzilla-cli version. fx-bug-toolkit PINS this crate, so the
  fx-bug-toolkit issue step is MANDATORY — a bump is not done until that issue is
  filed. Triggers on: "bump version", "/bump-version", "release bugzilla-cli",
  "cut a release", "publish bugzilla-cli".
allowed-tools: [Read, Edit, Bash, AskUserQuestion]
---

# bugzilla-cli Version Bump

fx-bug-toolkit **pins** bugzilla-cli: its `.claude-plugin/versions.json` plus
inline `cargo install bugzilla-cli --version <X>` pins (in `/triage` and
`/update`), enforced by a drift test. So every bump must be mirrored by an issue
asking fx-bug-toolkit to bump that pin.

**MANDATORY: the bump is not complete until the fx-bug-toolkit issue is filed
(Step 6). Do not report done before then.**

Downstream repo: **`alastor0325/fx-bug-toolkit`**.

---

## Step 1 — Decide the new version

```bash
grep -m1 '^version' Cargo.toml
```

Use the caller's level (`patch`/`minor`/`major`) or explicit version; otherwise
`AskUserQuestion`. Set `OLD` and `NEW`.

## Step 2 — Bump Cargo.toml

Edit only the `version` field under `[package]`, then refresh the lockfile:

```bash
cargo update -p bugzilla-cli   # updates Cargo.lock to the new version
grep -m1 '^version' Cargo.toml
```

## Step 3 — Test (green before committing — see CLAUDE.md)

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

A failing check is a hard blocker.

## Step 4 — Commit, tag, push

```bash
git add Cargo.toml Cargo.lock <other release files>
git commit -m "release: vNEW

<one-line summary>"
git tag vNEW
git push origin <current-branch>
git push origin vNEW
```

## Step 5 — Publish to crates.io

fx-bug-toolkit installs via `cargo install bugzilla-cli --version <X>`, so the new
version must be on crates.io:

```bash
cargo publish
```

(Needs `cargo login` once. If publishing isn't possible right now, say so — the
fx-bug-toolkit pin must NOT be bumped to a version that isn't on crates.io yet.)

## Step 6 — File the fx-bug-toolkit issue (MANDATORY)

```bash
PREV=$(git describe --tags --abbrev=0 vNEW^ 2>/dev/null || echo "")
[ -n "$PREV" ] && git log --oneline "$PREV"..vNEW || git log --oneline -10 vNEW
```

Write the body to a temp file, then:

```bash
gh issue create -R alastor0325/fx-bug-toolkit \
  --title "bump bugzilla-cli pin to vNEW" \
  --label enhancement \
  --body-file /tmp/bugzilla-cli-bump-issue.md && rm -f /tmp/bugzilla-cli-bump-issue.md
```

The body must include:
- The new version **NEW** and previous **OLD**, and a short changelog.
- The concrete ask: bump the pin to **NEW** in **all** these places (a drift test
  enforces they agree):
  - `.claude-plugin/versions.json` → `"bugzilla-cli"`
  - the inline `cargo install bugzilla-cli --version <X>` in `skills/triage/SKILL.md`
  - the same inline pin in `skills/update/SKILL.md`
- Confirm `cargo install bugzilla-cli --version NEW` resolves (it's on crates.io).
- Source: the commit hash and tag `vNEW`.

Report the created issue URL.

## Step 7 — Summary

old → new, pushed tag, crates.io publish result, changelog, and the fx-bug-toolkit
issue URL. If the issue was NOT filed, say so loudly — the release is incomplete.
