"""Tests for subcommand handlers."""

import json
import sys
import pytest

from bugzilla_cli import (
    cmd_get, cmd_fetch, cmd_post_comment, cmd_set_ni,
    cmd_set_fields, cmd_watch_add, cmd_watch_remove, cmd_watch_poll,
    WatchList,
)


class FakeArgs:
    def __init__(self, **kwargs):
        self.__dict__.update(kwargs)


def test_cmd_get_prints_summary(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", comments=False)
    cmd_get(args, client)
    out = capsys.readouterr().out
    assert "Test bug: HEVC fails on MKV" in out
    assert "P2" in out


def test_cmd_get_with_comments(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", comments=True)
    cmd_get(args, client)
    out = capsys.readouterr().out
    assert "reporter@example.com" in out


def test_cmd_fetch_outputs_json(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(start=None, end=None)
    cmd_fetch(args, client)
    out = capsys.readouterr().out
    bugs = json.loads(out.strip())
    assert isinstance(bugs, list)
    assert bugs[0]["id"] == 2027000


def test_cmd_fetch_end_filter(mocked_bmo, capsys):
    _, client = mocked_bmo
    # end date before the bug's creation_time → filtered out
    args = FakeArgs(start=None, end="2026-03-31")
    cmd_fetch(args, client)
    out = capsys.readouterr().out
    bugs = json.loads(out.strip())
    assert len(bugs) == 0


def test_cmd_post_comment(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", text="Need more info.")
    cmd_post_comment(args, client)
    out = capsys.readouterr().out
    assert "999" in out


def test_cmd_set_ni_single(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", email=["dev@example.com"])
    cmd_set_ni(args, client)
    out = capsys.readouterr().out
    assert "dev@example.com" in out


def test_cmd_set_ni_multiple(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", email=["a@example.com", "b@example.com"])
    cmd_set_ni(args, client)
    out = capsys.readouterr().out
    assert "a@example.com" in out
    assert "b@example.com" in out


def test_cmd_set_fields_priority(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", priority="P1", severity=None,
                    resolution=None, blocks_add=None, keywords_add=None)
    cmd_set_fields(args, client)
    out = capsys.readouterr().out
    assert "priority" in out


def test_cmd_set_fields_nothing(mocked_bmo, capsys):
    _, client = mocked_bmo
    args = FakeArgs(id="2027000", priority=None, severity=None,
                    resolution=None, blocks_add=None, keywords_add=None)
    cmd_set_fields(args, client)
    out = capsys.readouterr().out
    assert "Nothing" in out


def test_cmd_watch_add(mocked_bmo, tmp_path, monkeypatch, capsys):
    _, client = mocked_bmo
    watch_file = tmp_path / "ni-watch.json"
    monkeypatch.setattr("bugzilla_cli.WATCH_FILE", watch_file)
    monkeypatch.setattr("bugzilla_cli.TRIAGE_DIR", tmp_path)
    args = FakeArgs(id="2027000", title="Test bug", ni=["dev@example.com"])
    cmd_watch_add(args, client)
    wl = WatchList(watch_file)
    assert "2027000" in wl.all()


def test_cmd_watch_remove(mocked_bmo, tmp_path, monkeypatch, capsys):
    _, client = mocked_bmo
    watch_file = tmp_path / "ni-watch.json"
    monkeypatch.setattr("bugzilla_cli.WATCH_FILE", watch_file)
    monkeypatch.setattr("bugzilla_cli.TRIAGE_DIR", tmp_path)
    wl = WatchList(watch_file)
    wl.add(2027000, "Test", ["dev@example.com"], "2026-04-01T10:00:00Z")
    args = FakeArgs(id="2027000")
    cmd_watch_remove(args, client)
    assert "2027000" not in WatchList(watch_file).all()


def test_cmd_watch_poll(mocked_bmo, tmp_path, monkeypatch, capsys):
    _, client = mocked_bmo
    watch_file = tmp_path / "ni-watch.json"
    monkeypatch.setattr("bugzilla_cli.WATCH_FILE", watch_file)
    monkeypatch.setattr("bugzilla_cli.TRIAGE_DIR", tmp_path)
    wl = WatchList(watch_file)
    wl.add(2027000, "Test", ["reporter@example.com"], "2026-04-01T09:00:00Z")
    args = FakeArgs()
    cmd_watch_poll(args, client)
    out = capsys.readouterr().out
    result = json.loads(out)
    assert "2027000" in result["replied"]
