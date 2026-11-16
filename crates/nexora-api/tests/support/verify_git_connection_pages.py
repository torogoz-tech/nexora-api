"""Observe real API pagination without exposing credentials or relying on item order."""
from urllib.parse import urlencode


def verify_git_connection_pages(request, credential, expected_ids):
    visited = []
    cursor = None
    for _ in range(len(expected_ids) + 1):
        query = {"limit": 1}
        if cursor:
            query["after"] = cursor
        status, page, _ = request("/api/v1/git/connections?" + urlencode(query), token=credential)
        assert status == 200 and len(page["items"]) <= 1, "Connection page ignored its limit"
        visited.extend(item["id"] for item in page["items"])
        after = page["next_cursor"]
        if after is None:
            break
        assert page["items"] and after == page["items"][-1]["id"] and (cursor is None or after > cursor), "Connection cursor failed to advance"
        cursor = after
    else:
        raise AssertionError("Connection pagination did not terminate")
    assert len(visited) == len(set(visited)) and set(visited) == set(expected_ids), "Connection pagination duplicated or lost visible items"
    for query in ["limit=0", "limit=101", "limit=bad", "after=prj_" + "0" * 32, "actor=forged"]:
        assert request("/api/v1/git/connections?" + query, token=credential)[0] == 422, "Connection query accepted invalid bounds/cursor/authority"
