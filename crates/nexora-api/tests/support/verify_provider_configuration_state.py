"""Observe administrative state independently of encrypted provider credentials."""


def verify_provider_configuration_state(request, route, owner, denied, connection_id, bindings):
    body = {'enabled': True, 'expected_revision': 1}
    status, same, _ = request(route, body, owner, method='PATCH')
    assert status == 200 and same['revision'] == 1, 'Provider state endpoint missing or no-op changed revision'
    assert request(route, body, method='PATCH')[0] == 401, 'Unauthenticated caller changed provider state'
    assert request(route, body, denied, method='PATCH')[0] == 403, 'Non-administrator changed provider state'
    assert request(route, {**body, 'client_secret': 'forbidden-input'}, owner, method='PATCH')[0] == 422, 'State endpoint accepted credential material'
    disabled = {'enabled': False, 'expected_revision': 1}
    status, saved, _ = request(route, disabled, owner, method='PATCH', extra_headers={'Idempotency-Key': 'state-disable-once'})
    assert status == 200 and saved['revision'] == 2 and saved['enabled'] is False, 'Provider disable failed'
    assert not any(k in saved for k in ['secrets', 'client_secret', 'private_key_pem', 'webhook_secret', 'credential_revision']), 'State change exposed credential data'
    connections = request('/api/v1/git/connections', token=owner)[1]['items']
    assert next(c for c in connections if c['id'] == connection_id)['status'] == 'revoked', 'Disable retained connection authority'
    for project, repository in bindings:
        assert request(f'/api/v1/projects/{project}/repositories/{repository}', token=owner)[1]['authorization_status'] == 'revoked', 'Disable retained repository authority'
    assert request(route, {'enabled': True, 'expected_revision': 1}, owner, method='PATCH')[0] == 409, 'State change ignored stale revision'
    status, enabled, _ = request(route, {'enabled': True, 'expected_revision': 2}, owner, method='PATCH')
    assert status == 200 and enabled['revision'] == 3 and enabled['enabled'] is True, 'Provider re-enable failed'
    connections = request('/api/v1/git/connections', token=owner)[1]['items']
    assert next(c for c in connections if c['id'] == connection_id)['status'] == 'revoked', 'Re-enable restored a revoked connection'
    for project, repository in bindings:
        assert request(f'/api/v1/projects/{project}/repositories/{repository}', token=owner)[1]['authorization_status'] == 'revoked', 'Re-enable restored a revoked binding'
