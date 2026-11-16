"""Exercise actual provider IO, temporary failure and in-flight permission withdrawal."""
from concurrent.futures import ThreadPoolExecutor


def verify_remote_discovery_revocation(request, route, owner, delegate, delegation_route, connection_id, control):
    control['status'] = 503
    try:
        assert request(route + '?page=1&limit=1', token=owner)[0] == 503, 'Temporary provider failure was hidden'
        connections = request('/api/v1/git/connections', token=owner)[1]['items']
        assert next(item for item in connections if item['id'] == connection_id)['status'] == 'active', 'Temporary discovery failure revoked source authority'
    finally:
        control['status'] = 200
    control['entered'].clear()
    control['release'].clear()
    control['pause'] = True
    with ThreadPoolExecutor(max_workers=1) as pool:
        pending = pool.submit(request, route + '?page=1&limit=1', token=delegate)
        try:
            assert control['entered'].wait(timeout=20), 'Discovery did not reach provider gate'
            assert request(delegation_route, token=owner, method='DELETE')[0] == 204, 'Concurrent discovery delegation withdrawal failed'
        finally:
            control['pause'] = False
            control['release'].set()
        status, body, _ = pending.result(timeout=30)
        assert status == 403 and 'items' not in body, 'Discovery returned private metadata after delegation withdrawal'
    control['status'] = 401
    try:
        assert request(route + '?page=1&limit=1', token=owner)[0] == 401, 'Rejected provider credential remained usable for discovery'
        connections = request('/api/v1/git/connections', token=owner)[1]['items']
        assert next(item for item in connections if item['id'] == connection_id)['status'] == 'revoked', 'Provider credential rejection did not invalidate connection'
    finally:
        control['status'] = 200
