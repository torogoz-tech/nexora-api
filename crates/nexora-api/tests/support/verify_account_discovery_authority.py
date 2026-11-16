"""Global repository discovery remains bounded by current connection-owner credentials."""
from concurrent.futures import ThreadPoolExecutor


def verify_account_discovery_authority(request, route, owner, owner_id, control):
    endpoint = f'/api/v1/actors/{owner_id}/api-keys'
    status, issued, _ = request(endpoint, {
        'name': 'Account catalog verification',
        'scope': {'capabilities': ['connections:manage'], 'project_ids': [], 'all_projects': False, 'installation': True},
        'expires_in_seconds': 3600,
    }, owner)
    assert status == 201, 'Account discovery key setup failed'
    key = issued['key']
    assert request(route, token=key)[0] == 200, 'Connection owner required a project to discover repositories'
    control['entered'].clear()
    control['release'].clear()
    control['pause'] = True
    with ThreadPoolExecutor(max_workers=1) as pool:
        pending = pool.submit(request, route + '?page=1&limit=1', token=key)
        try:
            assert control['entered'].wait(timeout=20), 'Account discovery did not reach provider gate'
            assert request('/api/v1/api-keys/' + issued['metadata']['id'], token=owner, method='DELETE')[0] == 204, 'Account discovery key revocation failed'
        finally:
            control['pause'] = False
            control['release'].set()
        status, body, _ = pending.result(timeout=30)
        assert status == 401 and 'items' not in body, 'Global discovery returned private metadata after credential revocation'
