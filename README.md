# bugzilla-cli

Thin BMO REST client for Firefox A/V triage. No third-party Bugzilla libraries — just `requests` and your API key.

## Install

```bash
pip install -r requirements.txt
# Make the script executable and on PATH:
chmod +x bugzilla_cli.py
ln -s $(pwd)/bugzilla_cli.py ~/.local/bin/bugzilla-cli
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
bugzilla-cli get <id>                          # show bug + comments
bugzilla-cli fetch [--start YYYY-MM-DD] [--end YYYY-MM-DD]
bugzilla-cli post-comment <id> <text>
bugzilla-cli set-ni <id> <email>...            # one or many NI flags
bugzilla-cli set-fields <id> [--priority P1] [--severity S2] [--blocks-add 123] [--keywords-add stalled]
bugzilla-cli apply <id>                        # apply pending/bug-{id}.json draft
bugzilla-cli watch-add <id> --title "…" --ni <email>...
bugzilla-cli watch-remove <id>
bugzilla-cli watch-poll                        # JSON: {replied, stale, removed}
```

## Development

```bash
pip install -r requirements-dev.txt
pre-commit install

make test        # unit tests only (fast)
make test-all    # include integration tests
make lint
make format
```

TDD flow: write a failing test, run `make test` to confirm red, implement, confirm green.

Pre-commit hooks enforce:
- `ruff` lint + format on every commit
- Unit tests must pass before commit succeeds
- No hardcoded API keys in source

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
