# bugzilla-cli

Thin BMO REST client. Written in Rust using `ureq` for HTTP and `clap` for the CLI.

## Modes

bugzilla-cli has two modes:

- **Read-only (default, no API key).** `get`, `fetch`, `search`, and `watch-poll`
  work anonymously against BMO's public REST API — no setup required. Caveats:
  **security-restricted bugs are not visible**, and anonymous requests are
  rate-limited.
- **Write / reply mode (API key).** Adds `post-comment`, `set-ni`, `set-fields`,
  and `apply`. Enable it with `bugzilla-cli setup` (choose write mode and provide
  a key).

A configured key is also used for reads, so a write-mode user automatically sees
private bugs and gets higher rate limits.

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

Setup is **only needed for write/reply mode** — reads work without it.

```bash
bugzilla-cli setup
```

The wizard:
1. Prompts for your BMO base URL.
2. Asks whether to enable **write operations**:
   - **No (read-only):** skips the API key — only the triage directory is created.
   - **Yes (write):** prompts for an API key and verifies it with `GET /rest/whoami`.
3. Creates `~/firefox-triage/{bugs,pending,reports,archive,knowledge}/`.
4. In write mode, writes `~/.config/triage/secrets` (chmod 600) with `export BUGZILLA_BOT_API_KEY=...`.

In write mode, add `source ~/.config/triage/secrets` to your `~/.zshrc`.

## Commands

### Identity

| Command | Description |
|---------|-------------|
| `bugzilla-cli whoami` | Print the BMO login (email) tied to the stored API key |
| `bugzilla-cli setup` | Interactive wizard: choose read-only or write mode, (write only) API key + secrets file, triage directory |
| `bugzilla-cli version` | Print version and git commit hash |
| `bugzilla-cli update` | Update to the latest version — `git pull + cargo build` for dev symlink installs, `cargo install --git <repo>` for `cargo install` installs |
| `bugzilla-cli --version` | Print version (short form) |

### Reading bugs

These work **without an API key** (public bugs only). A configured key adds
private-bug visibility and higher rate limits.

| Command | Description |
|---------|-------------|
| `bugzilla-cli get <id>` | Show bug metadata and full comment thread |
| `bugzilla-cli get <id> --no-comments` | Show bug metadata only |
| `bugzilla-cli fetch` | Fetch triage-queue bugs from the current ISO week (default component set) |
| `bugzilla-cli fetch --start YYYY-MM-DD --end YYYY-MM-DD` | Fetch bugs in a custom date range |
| `bugzilla-cli fetch --component <comp>...` | Fetch only these components (repeatable); the caller owns the list |
| `bugzilla-cli search <query>` | Search open bugs by summary substring (default: up to 25 results) |
| `bugzilla-cli search <query> --component <comp>` | Narrow to one or more components (flag is repeatable) |
| `bugzilla-cli search <query> --full-text` | Also search comments and descriptions |
| `bugzilla-cli search <query> --all-statuses` | Include resolved/closed bugs |
| `bugzilla-cli search <query> --limit <n>` | Cap result count |

### Writing bugs

These **require an API key** — run `bugzilla-cli setup` and choose write mode.

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

### fetch examples

```bash
bugzilla-cli fetch                                   # default A/V component set, current ISO week
bugzilla-cli fetch --component "Audio/Video: Playback" --component "Web Audio"
bugzilla-cli fetch --start 2026-05-01 --component "Audio/Video: GMP"
```

`fetch` does not own the component list: pass `--component` (repeatable) to scope
it however the caller wants. When no `--component` is given it falls back to a
built-in default A/V set so bare `fetch` still works standalone.

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
