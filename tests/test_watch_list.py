"""Tests for WatchList."""

import json
import pytest

from bugzilla_cli import WatchList


@pytest.fixture
def watch_file(tmp_path):
    return tmp_path / "ni-watch.json"


@pytest.fixture
def wl(watch_file):
    return WatchList(watch_file)


def test_add_creates_entry(wl, watch_file):
    wl.add(2027000, "Test bug", ["dev@example.com"], "2026-04-01T10:00:00Z")
    data = json.loads(watch_file.read_text())
    assert "2027000" in data
    assert data["2027000"]["title"] == "Test bug"
    assert data["2027000"]["ni_targets"] == ["dev@example.com"]


def test_add_multiple_ni_targets(wl):
    wl.add(2027000, "Multi NI", ["a@example.com", "b@example.com"], "2026-04-01T10:00:00Z")
    entry = wl.all()["2027000"]
    assert len(entry["ni_targets"]) == 2


def test_remove_existing_entry(wl):
    wl.add(2027000, "Test", ["x@example.com"], "2026-04-01T10:00:00Z")
    removed = wl.remove(2027000)
    assert removed is True
    assert "2027000" not in wl.all()


def test_remove_nonexistent_returns_false(wl):
    assert wl.remove(9999999) is False


def test_all_returns_dict(wl):
    wl.add(1, "Bug 1", ["a@example.com"], "2026-04-01T10:00:00Z")
    wl.add(2, "Bug 2", ["b@example.com"], "2026-04-01T10:00:00Z")
    all_bugs = wl.all()
    assert set(all_bugs.keys()) == {"1", "2"}


def test_persists_across_instances(watch_file):
    wl1 = WatchList(watch_file)
    wl1.add(2027000, "Persistent", ["p@example.com"], "2026-04-01T10:00:00Z")

    wl2 = WatchList(watch_file)
    assert "2027000" in wl2.all()


def test_poll_detects_reply(wl, mocked_bmo):
    _, client = mocked_bmo
    # NI set before the comment was posted
    wl.add(2027000, "Test", ["reporter@example.com"], "2026-04-01T09:00:00Z")
    result = wl.poll(client)
    assert "2027000" in result["replied"]
    assert "2027000" not in wl.all()


def test_poll_no_reply_not_stale(wl, mocked_bmo):
    _, client = mocked_bmo
    # NI set after the comment — not a reply
    wl.add(2027000, "Test", ["reporter@example.com"], "2026-04-02T00:00:00Z")
    result = wl.poll(client)
    assert "2027000" not in result["replied"]


def test_poll_stale_after_7_days(wl, mocked_bmo):
    _, client = mocked_bmo
    # NI set 8 days ago, no reply
    wl.add(2027000, "Test", ["nobody@example.com"], "2026-03-24T00:00:00Z")
    result = wl.poll(client)
    assert "2027000" in result["stale"]
