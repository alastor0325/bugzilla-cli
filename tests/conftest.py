import pytest
import responses as resp_lib

from bugzilla_cli import BmoClient

BMO_BASE = "https://bugzilla.mozilla.org/rest"


@pytest.fixture
def api_key():
    return "test-api-key-abc123"


@pytest.fixture
def client(api_key):
    return BmoClient(api_key)


FAKE_BUG = {
    "id": 2027000,
    "summary": "Test bug: HEVC fails on MKV",
    "status": "NEW",
    "resolution": "",
    "priority": "P2",
    "severity": "S3",
    "assigned_to": "nobody@mozilla.org",
    "creation_time": "2026-04-01T10:00:00Z",
    "flags": [],
}

FAKE_COMMENT = {
    "id": 100,
    "text": "This is a test comment.",
    "creator": "reporter@example.com",
    "creation_time": "2026-04-01T10:05:00Z",
}


@pytest.fixture
def mocked_bmo(api_key):
    with resp_lib.RequestsMock(assert_all_requests_are_fired=False) as rsps:
        rsps.add(
            resp_lib.GET,
            f"{BMO_BASE}/whoami",
            json={"id": 42, "name": "bot@mozilla.com", "real_name": "Triage Bot"},
        )
        rsps.add(
            resp_lib.GET,
            f"{BMO_BASE}/bug/2027000",
            json={"bugs": [FAKE_BUG]},
        )
        rsps.add(
            resp_lib.GET,
            f"{BMO_BASE}/bug/2027000/comment",
            json={"bugs": {"2027000": {"comments": [FAKE_COMMENT]}}},
        )
        rsps.add(
            resp_lib.GET,
            f"{BMO_BASE}/bug",
            json={"bugs": [FAKE_BUG]},
        )
        rsps.add(
            resp_lib.POST,
            f"{BMO_BASE}/bug/2027000/comment",
            json={"id": 999},
        )
        rsps.add(
            resp_lib.PUT,
            f"{BMO_BASE}/bug/2027000",
            json={"bugs": [{"id": 2027000, "last_change_time": "2026-04-02T00:00:00Z"}]},
        )
        yield rsps, BmoClient(api_key)
