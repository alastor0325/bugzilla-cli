#!/usr/bin/env python3
"""bugzilla-cli — thin BMO REST client for Firefox A/V triage."""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

import requests

BMO_BASE = "https://bugzilla.mozilla.org/rest"
TRIAGE_DIR = Path.home() / "firefox-triage"
WATCH_FILE = TRIAGE_DIR / "ni-watch.json"


# ---------------------------------------------------------------------------
# BmoClient
# ---------------------------------------------------------------------------

class BmoClient:
    def __init__(self, api_key: str, base_url: str = BMO_BASE) -> None:
        self._base = base_url.rstrip("/")
        self._session = requests.Session()
        self._session.headers.update({"X-BUGZILLA-API-KEY": api_key})

    def get(self, path: str, **params) -> dict:
        r = self._session.get(f"{self._base}/{path.lstrip('/')}", params=params)
        r.raise_for_status()
        return r.json()

    def post(self, path: str, body: dict) -> dict:
        r = self._session.post(f"{self._base}/{path.lstrip('/')}", json=body)
        r.raise_for_status()
        return r.json()

    def put(self, path: str, body: dict) -> dict:
        r = self._session.put(f"{self._base}/{path.lstrip('/')}", json=body)
        r.raise_for_status()
        return r.json()

    def whoami(self) -> dict:
        return self.get("/whoami")

    def get_bug(self, bug_id: int | str, include_comments: bool = True) -> dict:
        bug = self.get(f"/bug/{bug_id}")
        result = {"bug": bug["bugs"][0]}
        if include_comments:
            comments = self.get(f"/bug/{bug_id}/comment")
            result["comments"] = comments["bugs"][str(bug_id)]["comments"]
        return result

    def search(self, **params) -> list:
        data = self.get("/bug", **params)
        return data.get("bugs", [])


# ---------------------------------------------------------------------------
# WatchList
# ---------------------------------------------------------------------------

class WatchList:
    def __init__(self, watch_file: Path = WATCH_FILE) -> None:
        self._file = watch_file
        self._data: dict[str, dict] = {}
        if self._file.exists():
            self._data = json.loads(self._file.read_text())

    def _save(self) -> None:
        self._file.parent.mkdir(parents=True, exist_ok=True)
        self._file.write_text(json.dumps(self._data, indent=2))

    def add(self, bug_id: int | str, title: str, ni_targets: list[str], ni_set_date: str) -> None:
        self._data[str(bug_id)] = {
            "title": title,
            "ni_targets": ni_targets,
            "ni_set_date": ni_set_date,
        }
        self._save()

    def remove(self, bug_id: int | str) -> bool:
        key = str(bug_id)
        if key not in self._data:
            return False
        del self._data[key]
        self._save()
        return True

    def all(self) -> dict[str, dict]:
        return dict(self._data)

    def poll(self, client: BmoClient) -> dict:
        replied: list[str] = []
        stale: list[str] = []
        removed: list[str] = []

        for bug_id, entry in list(self._data.items()):
            try:
                data = client.get_bug(bug_id, include_comments=True)
            except requests.HTTPError:
                removed.append(bug_id)
                continue

            ni_date = datetime.fromisoformat(entry["ni_set_date"].replace("Z", "+00:00"))
            comments = data.get("comments", [])
            targets = set(entry.get("ni_targets", []))

            responders = {
                c["creator"]
                for c in comments
                if datetime.fromisoformat(c["creation_time"].replace("Z", "+00:00")) > ni_date
                and c["creator"] in targets
            }

            if responders:
                replied.append(bug_id)
                self.remove(bug_id)
            else:
                age_days = (datetime.now(timezone.utc) - ni_date).days
                if age_days >= 7:
                    stale.append(bug_id)

        return {"replied": replied, "stale": stale, "removed": removed}


# ---------------------------------------------------------------------------
# Subcommand implementations
# ---------------------------------------------------------------------------

def cmd_setup(args, client=None) -> None:
    print("=== bugzilla-cli setup ===")

    url = input(f"BMO base URL [{BMO_BASE}]: ").strip() or BMO_BASE
    api_key = input("API key (will not be stored here): ").strip()
    if not api_key:
        print("ERROR: API key is required.")
        sys.exit(1)

    test_client = BmoClient(api_key, url)
    print("Verifying API key with BMO...")
    try:
        me = test_client.whoami()
        print(f"Authenticated as: {me.get('real_name', '?')} <{me.get('name', '?')}>")
    except requests.HTTPError as e:
        print(f"ERROR: Authentication failed: {e}")
        sys.exit(1)

    triage_dir = Path(input(f"Triage directory [{TRIAGE_DIR}]: ").strip() or TRIAGE_DIR)
    for sub in ("bugs", "pending", "reports", "archive"):
        (triage_dir / sub).mkdir(parents=True, exist_ok=True)
    print(f"Created triage directories under {triage_dir}")

    secrets_file = Path.home() / ".config" / "triage" / "secrets"
    secrets_file.parent.mkdir(parents=True, exist_ok=True)
    secrets_file.write_text(f"export BUGZILLA_BOT_API_KEY={api_key}\n")
    secrets_file.chmod(0o600)
    print(f"API key written to {secrets_file} (chmod 600)")

    print()
    print("Add this to your ~/.zshrc:")
    print(f"  source {secrets_file}")
    print()
    print("Setup complete.")


def cmd_get(args, client: BmoClient) -> None:
    data = client.get_bug(args.id, include_comments=True)
    bug = data["bug"]
    print(f"Bug {bug['id']}: {bug['summary']}")
    print(f"  Status:   {bug['status']} {bug.get('resolution', '')}".rstrip())
    print(f"  Priority: {bug.get('priority', '?')}  Severity: {bug.get('severity', '?')}")
    print(f"  Assigned: {bug.get('assigned_to', '?')}")
    if args.comments:
        comments = data.get("comments", [])
        print(f"\n--- {len(comments)} comment(s) ---")
        for c in comments:
            print(f"\n[{c['creation_time']}] {c['creator']}:")
            print(c["text"][:500] + ("..." if len(c["text"]) > 500 else ""))


def cmd_fetch(args, client: BmoClient) -> None:
    params: dict = {"savedsearch": "media-meta", "include_fields": "_default"}
    if args.start:
        params["creation_time"] = args.start

    bugs = client.search(**params)

    if args.end:
        end_dt = datetime.fromisoformat(args.end)
        bugs = [
            b for b in bugs
            if datetime.fromisoformat(b["creation_time"].replace("Z", "")) <= end_dt
        ]

    bugs.sort(key=lambda b: b["creation_time"])
    print(json.dumps(bugs, indent=2))
    print(f"\n# {len(bugs)} bug(s) fetched", file=sys.stderr)


def cmd_post_comment(args, client: BmoClient) -> None:
    body = {"comment": args.text}
    result = client.post(f"/bug/{args.id}/comment", body)
    print(f"Comment {result['id']} posted to bug {args.id}.")


def cmd_set_ni(args, client: BmoClient) -> None:
    flags = [
        {"name": "needinfo", "status": "?", "requestee": email}
        for email in args.email
    ]
    client.put(f"/bug/{args.id}", {"flags": flags})
    print(f"NI set on bug {args.id} for: {', '.join(args.email)}")


def cmd_set_fields(args, client: BmoClient) -> None:
    body: dict = {}
    if args.priority:
        body["priority"] = args.priority
    if args.severity:
        body["severity"] = args.severity
    if args.resolution:
        body["resolution"] = args.resolution
    if args.blocks_add:
        body["blocks"] = {"add": args.blocks_add}
    if args.keywords_add:
        body["keywords"] = {"add": args.keywords_add}
    if not body:
        print("Nothing to update.")
        return
    client.put(f"/bug/{args.id}", body)
    print(f"Bug {args.id} updated: {list(body.keys())}")


def cmd_apply(args, client: BmoClient) -> None:
    pending_file = Path(str(TRIAGE_DIR)) / "pending" / f"bug-{args.id}.json"
    if not pending_file.exists():
        print(f"ERROR: No pending draft for bug {args.id} at {pending_file}")
        sys.exit(1)

    draft = json.loads(pending_file.read_text())
    print(f"--- Draft for bug {draft['bug_id']}: {draft['title']} ---")
    print(f"Comment:\n{draft['comment']}")
    print(f"NI targets: {draft.get('ni_targets', [])}")
    print(f"Fields: priority={draft.get('priority')}, severity={draft.get('severity')}, "
          f"blocks_add={draft.get('blocks_add', [])}, keywords_add={draft.get('keywords_add', [])}")

    confirm = input("\nApply? [y/N] ").strip().lower()
    if confirm != "y":
        print("Aborted.")
        return

    if draft.get("comment"):
        client.post(f"/bug/{draft['bug_id']}/comment", {"comment": draft["comment"]})

    field_body: dict = {}
    if draft.get("priority"):
        field_body["priority"] = draft["priority"]
    if draft.get("severity"):
        field_body["severity"] = draft["severity"]
    if draft.get("resolution"):
        field_body["resolution"] = draft["resolution"]
    if draft.get("blocks_add"):
        field_body["blocks"] = {"add": draft["blocks_add"]}
    if draft.get("keywords_add"):
        field_body["keywords"] = {"add": draft["keywords_add"]}
    if field_body:
        client.put(f"/bug/{draft['bug_id']}", field_body)

    if draft.get("ni_targets"):
        now = datetime.now(timezone.utc).isoformat()
        WatchList(WATCH_FILE).add(draft["bug_id"], draft["title"], draft["ni_targets"], now)
        flags = [
            {"name": "needinfo", "status": "?", "requestee": e}
            for e in draft["ni_targets"]
        ]
        client.put(f"/bug/{draft['bug_id']}", {"flags": flags})

    pending_file.unlink()
    print(f"Applied. Draft removed.")


def cmd_watch_add(args, client: BmoClient) -> None:
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    WatchList(WATCH_FILE).add(args.id, args.title, args.ni, now)
    print(f"Watching bug {args.id}.")


def cmd_watch_remove(args, client: BmoClient) -> None:
    removed = WatchList(WATCH_FILE).remove(args.id)
    if removed:
        print(f"Removed bug {args.id} from watch list.")
    else:
        print(f"Bug {args.id} was not in watch list.")


def cmd_watch_poll(args, client: BmoClient) -> None:
    result = WatchList(WATCH_FILE).poll(client)
    print(json.dumps(result, indent=2))


# ---------------------------------------------------------------------------
# CLI wiring
# ---------------------------------------------------------------------------

def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="bugzilla-cli",
        description="Thin BMO REST client for Firefox A/V triage.",
    )
    sub = p.add_subparsers(dest="command", required=True)

    sub.add_parser("setup", help="Interactive setup wizard")

    g = sub.add_parser("get", help="Fetch a single bug")
    g.add_argument("id", help="Bug ID")
    g.add_argument("--no-comments", dest="comments", action="store_false", default=True)

    f = sub.add_parser("fetch", help="Fetch bugs from saved search 'media-meta'")
    f.add_argument("--start", metavar="YYYY-MM-DD", help="Lower bound (creation_time >=)")
    f.add_argument("--end", metavar="YYYY-MM-DD", help="Upper bound (filtered in Python)")

    pc = sub.add_parser("post-comment", help="Post a comment to a bug")
    pc.add_argument("id", help="Bug ID")
    pc.add_argument("text", help="Comment text")

    ni = sub.add_parser("set-ni", help="Set needinfo flag(s) on a bug")
    ni.add_argument("id", help="Bug ID")
    ni.add_argument("email", nargs="+", help="Requestee email(s)")

    sf = sub.add_parser("set-fields", help="Update bug fields")
    sf.add_argument("id", help="Bug ID")
    sf.add_argument("--priority", choices=["P1", "P2", "P3", "P4", "P5", "--"])
    sf.add_argument("--severity", choices=["S1", "S2", "S3", "S4", "--"])
    sf.add_argument("--resolution")
    sf.add_argument("--blocks-add", nargs="+", type=int, metavar="BUG_ID")
    sf.add_argument("--keywords-add", nargs="+", metavar="KEYWORD")

    ap = sub.add_parser("apply", help="Apply a pending draft")
    ap.add_argument("id", help="Bug ID")

    wa = sub.add_parser("watch-add", help="Add a bug to the NI watch list")
    wa.add_argument("id", help="Bug ID")
    wa.add_argument("--title", required=True)
    wa.add_argument("--ni", nargs="+", required=True, metavar="EMAIL")

    wr = sub.add_parser("watch-remove", help="Remove a bug from the watch list")
    wr.add_argument("id", help="Bug ID")

    sub.add_parser("watch-poll", help="Poll watched bugs for NI replies")

    return p


def _get_client(command: str) -> BmoClient | None:
    if command == "setup":
        return None
    api_key = os.environ.get("BUGZILLA_BOT_API_KEY", "")
    if not api_key:
        print("ERROR: BUGZILLA_BOT_API_KEY is not set. Run 'bugzilla-cli setup' first.")
        sys.exit(1)
    return BmoClient(api_key)


HANDLERS = {
    "setup": cmd_setup,
    "get": cmd_get,
    "fetch": cmd_fetch,
    "post-comment": cmd_post_comment,
    "set-ni": cmd_set_ni,
    "set-fields": cmd_set_fields,
    "apply": cmd_apply,
    "watch-add": cmd_watch_add,
    "watch-remove": cmd_watch_remove,
    "watch-poll": cmd_watch_poll,
}


def main() -> None:
    parser = _build_parser()
    args = parser.parse_args()
    client = _get_client(args.command)
    HANDLERS[args.command](args, client)


if __name__ == "__main__":
    main()
