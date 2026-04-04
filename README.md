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

### Identity

| Command | Description |
|---------|-------------|
| `bugzilla-cli whoami` | Print the BMO login (email) tied to the stored API key |
| `bugzilla-cli setup` | Interactive wizard: API key, triage directory, secrets file |

### Reading bugs

| Command | Description |
|---------|-------------|
| `bugzilla-cli get <id>` | Show bug metadata and full comment thread |
| `bugzilla-cli get <id> --no-comments` | Show bug metadata only |
| `bugzilla-cli fetch` | Fetch triage-queue bugs from the current ISO week |
| `bugzilla-cli fetch --start YYYY-MM-DD --end YYYY-MM-DD` | Fetch bugs in a custom date range |
| `bugzilla-cli search <query>` | Search open bugs by summary substring (default: up to 25 results) |
| `bugzilla-cli search <query> --component <comp>` | Narrow to one or more components (flag is repeatable) |
| `bugzilla-cli search <query> --full-text` | Also search comments and descriptions |
| `bugzilla-cli search <query> --all-statuses` | Include resolved/closed bugs |
| `bugzilla-cli search <query> --limit <n>` | Cap result count |

### Writing bugs

| Command | Description |
|---------|-------------|
| `bugzilla-cli post-comment <id> <text>` | Post a comment |
| `bugzilla-cli set-ni <id> <email>...` | Set needinfo flags (one PUT, multiple recipients) |
| `bugzilla-cli set-fields <id> [options]` | Update priority, severity, resolution, blocks, keywords |
| `bugzilla-cli apply <id>` | Apply a pending draft from `~/firefox-triage/pending/bug-{id}.json` |

`set-fields` options: `--priority P1-P5\|--`, `--severity S1-S4\|--`, `--status <STATUS>`, `--resolution <RES>`, `--dupe-of <id>`, `--blocks-add <id>...`, `--keywords-add <kw>...`, `--cc-add <email>...`

To close a bug: `bugzilla-cli set-fields <id> --status RESOLVED --resolution FIXED`
To mark duplicate: `bugzilla-cli set-fields <id> --status RESOLVED --resolution DUPLICATE --dupe-of <bug-id>`

### NI watch list

| Command | Description |
|---------|-------------|
| `bugzilla-cli watch-add <id> --title "..." --ni <email>...` | Start watching a bug for needinfo replies |
| `bugzilla-cli watch-remove <id>` | Stop watching a bug |
| `bugzilla-cli watch-poll` | Check all watched bugs; reports `replied`, `stale` (≥7 days), `removed` |

### search examples

```bash
bugzilla-cli search "mp4 crash"
bugzilla-cli search "seek" --component "Audio/Video: Playback"
bugzilla-cli search "decode" --component "Audio/Video: Playback" --component "Audio/Video: Web Codecs"
bugzilla-cli search "NS_ERROR_FAILURE" --full-text
bugzilla-cli search "mp4 crash" --all-statuses --limit 50
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
cargo test --lib --bins   # unit tests only (fast, no network)
cargo test                # all tests including integration
cargo clippy              # lint
cargo fmt                 # format
make check                # lint + unit tests together
```

TDD flow: write a failing test, run `cargo test --lib --bins` to confirm red, implement, confirm green.

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
