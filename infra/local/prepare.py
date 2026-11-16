"""Prepare local infrastructure secrets without printing or replacing credentials."""
from pathlib import Path
import os
import secrets
import json

root = Path(__file__).resolve().parents[2]
directory = root / ".nexora/local/secrets"
directory.mkdir(parents=True, exist_ok=True, mode=0o700)
os.chmod(directory, 0o700)
values = {
    "postgres_password": secrets.token_hex(24) + "\n",
    "neo4j_auth": "neo4j/" + secrets.token_hex(24) + "\n",
    "dragonfly_flags": "\n".join([
        "--requirepass=" + secrets.token_hex(24),
        "--dir=/data",
        "--dbfilename=nexora",
        "--snapshot_cron=* * * * *",
        "--maxmemory=256mb",
        "--proactor_threads=1",
        "--cache_mode=false",
        "",
    ]),
}
for name, value in values.items():
    file = directory / name
    # Neo4j reads its secret after dropping root. The enclosing host directory
    # remains 0700; the mounted file must also be readable by its container UID.
    mode = 0o644 if name == "neo4j_auth" else 0o600
    try:
        descriptor = os.open(file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    except FileExistsError:
        if file.is_symlink() or not file.is_file():
            raise SystemExit(f"Refusing non-regular secret: {name}")
        os.chmod(file, mode)
        continue
    os.fchmod(descriptor, mode)
    with os.fdopen(descriptor, "w") as stream:
        stream.write(value)
print("Local credentials prepared; existing credentials preserved. No secrets printed.")

connections = directory.parent / "connections.json"
if not connections.exists():
    settings = {
        "schema_version": 1,
        "host": "127.0.0.1",
        "postgres_port": int(os.environ.get("NEXORA_POSTGRES_PORT", "15432")),
        "redis_port": int(os.environ.get("NEXORA_DRAGONFLY_PORT", "16379")),
        "neo4j_http_port": int(os.environ.get("NEXORA_NEO4J_HTTP_PORT", "17474")),
        "postgres_database": "nexora",
        "postgres_user": "nexora",
        "secrets_directory": "secrets",
    }
    with os.fdopen(os.open(connections, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as stream:
        json.dump(settings, stream, indent=2)
        stream.write("\n")
print("Nonsecret connection settings prepared; existing settings preserved.")
