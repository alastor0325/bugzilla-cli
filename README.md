# bugzilla-cli

Thin BMO REST client. Written in Rust using `ureq` for HTTP and `clap` for the CLI.

## Install

For coworkers (no checkout required):

```bash
cargo install --git https://github.com/alastor0325/bugzilla-cli
```

From source:

```bash
cargo install --path .
```

## Setup

```bash
bugzilla-cli setup
```

The wizard will:
1. Prompt for your BMO base URL and API key
2. Verify the key with `GET /rest/whoami`
3. Create `~/firefox-triage/{bugs,pending,reports,archive}/`
4. Write `~/.config/triage/secrets` (chmod 600) with `export BUGZILLA_BOT_API_KEY=...`

Add `source ~/.config/triage/secrets` to your `~/.zshrc`.

## Commands

```
bugzilla-cli get <id> [--no-comments]          # show bug + comments
bugzilla-cli fetch [--start YYYY-MM-DD] [--end YYYY-MM-DD]
                                               # default start = Monday of current ISO week
bugzilla-cli post-comment <id> <text>
bugzilla-cli set-ni <id> <email>...            # one or many NI flags in one PUT
bugzilla-cli set-fields <id> [--priority P1-P5|--] [--severity S1-S4|--]
                              [--resolution RES] [--blocks-add N...] [--keywords-add KW...]
bugzilla-cli apply <id>                        # apply pending/bug-{id}.json draft
bugzilla-cli watch-add <id> --title "..." --ni <email>...
bugzilla-cli watch-remove <id>
bugzilla-cli watch-poll                        # JSON: {replied, stale, removed}
```

## Development

### One-time setup

```bash
# Clone and enter the repo
git clone https://github.com/alastor0325/bugzilla-cli
cd bugzilla-cli

# Install dev hooks
pre-commit install

# Build and symlink the binary into PATH (no cargo install needed)
make install
```

`make install` symlinks `target/debug/bugzilla-cli` into `~/.local/bin/`. Make sure `~/.local/bin` is on your `PATH`.

### After fixing a bug

```bash
cargo build          # recompile — the symlink picks up the new binary automatically
bugzilla-cli ...     # immediately uses the updated binary
```

No `cargo install` or `make install` needed again.

### Other commands

```bash
cargo test --lib     # unit tests only (fast, no network)
cargo test           # all tests including integration
cargo clippy         # lint
cargo fmt            # format
make check           # lint + unit tests together
```

TDD flow: write a failing test, run `cargo test --lib` to confirm red, implement, confirm green.

Pre-commit hooks enforce:
- `cargo fmt` on every commit
- `cargo clippy -D warnings` on every commit
- Unit tests must pass before commit succeeds
- No hardcoded API keys in source

## Integration tests

Real BMO calls (read-only) are skipped by default. Run with:

```bash
BUGZILLA_BOT_API_KEY=your-key cargo test -- --ignored
```

## File layout

```
~/firefox-triage/
  bugs/           # fetched bug JSON snapshots
  pending/        # bug-{id}.json drafts for `apply`
  reports/        # weekly triage summaries
  archive/        # old reports
  ni-watch.json   # local NI watch state (not committed)
~/.config/triage/
  secrets         # chmod 600, export BUGZILLA_BOT_API_KEY=...
```

## Security

- API key is read from `$BUGZILLA_BOT_API_KEY` environment variable only — never stored in the repo.
- `~/.config/triage/secrets` is outside the repo and excluded by `.gitignore`.
- The `no-secrets` pre-commit hook rejects any commit that writes `API_KEY=` into source.
