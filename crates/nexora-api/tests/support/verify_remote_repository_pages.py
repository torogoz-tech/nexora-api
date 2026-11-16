"""Verify bounded metadata discovery through the real API, without logging its contents."""
from urllib.parse import urlencode


def verify_remote_repository_pages(request, route, credential, expected_ids):
    visited = []
    page = 1
    for _ in range(len(expected_ids) + 1):
        status, result, _ = request(route + '?' + urlencode({'page': page, 'limit': 1}), token=credential)
        assert status == 200, "Remote repository discovery API missing or inaccessible"
        assert len(result['items']) <= 1, "Remote repository discovery ignored page bounds"
        for item in result['items']:
            assert set(item) == {'provider', 'remote_id', 'full_name'}, "Discovery exposed credentials, provider URLs or unproven access"
            visited.append(item['remote_id'])
        next_page = result['next_page']
        if next_page is None:
            break
        assert next_page == page + 1, "Remote repository page failed to advance"
        page = next_page
    else:
        raise AssertionError("Remote repository pagination did not terminate")
    assert len(visited) == len(set(visited)) and set(visited) == set(expected_ids), "Discovery lost or duplicated repository identities"
    for query in ['page=0', 'page=100001', 'limit=0', 'limit=101', 'actor=forged', 'url=https://example.invalid']:
        assert request(route + '?' + query, token=credential)[0] == 422, "Discovery accepted invalid bounds or client authority/destination"
    assert request(route)[0] == 401, "Unauthenticated caller discovered private repositories"
