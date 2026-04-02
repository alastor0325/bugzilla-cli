"""Tests for BmoClient."""

import pytest
import responses

from bugzilla_cli import BmoClient

BMO_BASE = "https://bugzilla.mozilla.org/rest"


def test_client_sets_api_key_header(api_key, client):
    assert client._session.headers["X-BUGZILLA-API-KEY"] == api_key


def test_whoami_returns_user_info(mocked_bmo):
    _, c = mocked_bmo
    result = c.whoami()
    assert result["name"] == "bot@mozilla.com"
    assert result["real_name"] == "Triage Bot"


def test_get_bug_returns_bug_and_comments(mocked_bmo):
    _, c = mocked_bmo
    data = c.get_bug(2027000)
    assert data["bug"]["id"] == 2027000
    assert data["bug"]["summary"] == "Test bug: HEVC fails on MKV"
    assert len(data["comments"]) == 1
    assert data["comments"][0]["creator"] == "reporter@example.com"


def test_get_bug_no_comments(mocked_bmo):
    _, c = mocked_bmo
    data = c.get_bug(2027000, include_comments=False)
    assert "comments" not in data


def test_search_returns_bug_list(mocked_bmo):
    _, c = mocked_bmo
    bugs = c.search(savedsearch="media-meta")
    assert isinstance(bugs, list)
    assert bugs[0]["id"] == 2027000


def test_post_comment(mocked_bmo):
    _, c = mocked_bmo
    result = c.post("/bug/2027000/comment", {"comment": "Hello"})
    assert result["id"] == 999


def test_put_bug_fields(mocked_bmo):
    _, c = mocked_bmo
    result = c.put("/bug/2027000", {"priority": "P1"})
    assert "bugs" in result


@responses.activate
def test_http_error_raises(api_key):
    responses.add(responses.GET, f"{BMO_BASE}/whoami", status=401)
    c = BmoClient(api_key)
    with pytest.raises(Exception):
        c.whoami()


@responses.activate
def test_custom_base_url(api_key):
    custom_base = "https://bugzilla.example.com/rest"
    responses.add(responses.GET, f"{custom_base}/whoami", json={"name": "test"})
    c = BmoClient(api_key, base_url=custom_base)
    result = c.whoami()
    assert result["name"] == "test"
