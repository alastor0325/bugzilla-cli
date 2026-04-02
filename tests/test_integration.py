"""Integration tests — hit real BMO. Run with: pytest tests/test_integration.py --integration

Requires BUGZILLA_BOT_API_KEY to be set.
"""

import os
import pytest

from bugzilla_cli import BmoClient

BMO_BASE = "https://bugzilla.mozilla.org/rest"


def pytest_addoption(parser):
    parser.addoption("--integration", action="store_true", default=False)


@pytest.fixture(autouse=True)
def skip_unless_integration(request):
    if not request.config.getoption("--integration"):
        pytest.skip("pass --integration to run integration tests")


@pytest.fixture
def real_client():
    key = os.environ.get("BUGZILLA_BOT_API_KEY", "")
    if not key:
        pytest.skip("BUGZILLA_BOT_API_KEY not set")
    return BmoClient(key)


def test_whoami(real_client):
    me = real_client.whoami()
    assert "name" in me
    assert "@" in me["name"]


def test_get_public_bug(real_client):
    data = real_client.get_bug(2025302)
    assert data["bug"]["id"] == 2025302
    assert len(data["comments"]) >= 0


def test_search_media_meta(real_client):
    bugs = real_client.search(savedsearch="media-meta")
    assert isinstance(bugs, list)
