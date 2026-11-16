"""Expire only a generated local fixture connection; never touch installation data."""
import re
import subprocess


def expire_github_fixture_connection(namespace, connection):
    if not re.fullmatch(r"nexora_platform_github_[0-9]+", namespace):
        raise AssertionError("Refusing non-fixture namespace")
    if not re.fullmatch(r"gcn_[A-Za-z0-9_-]+", connection):
        raise AssertionError("Invalid fixture connection identifier")
    statement = (
        f'UPDATE "{namespace}".git_connections '
        "SET expires_at=now()-interval '1 second' "
        f"WHERE id='{connection}' AND provider='github' AND status='active' "
        "RETURNING credential_revision"
    )
    result = subprocess.run(
        ["docker", "exec", "nexora-local-postgres-1", "psql", "-X", "-U", "nexora", "-d", "nexora", "-At", "-v", "ON_ERROR_STOP=1", "-c", statement],
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 0 and result.stdout.splitlines() == ["1", "UPDATE 1"], "Fixture expiry was not applied exactly once"
