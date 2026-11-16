"""Probe the isolated Nexora services, optionally recreating containers to test durability."""
from pathlib import Path
import argparse
import base64
import json
import os
import subprocess
import sys
import time
import urllib.request
import uuid
from redis_probe import RedisProbe

ROOT = Path(__file__).resolve().parents[2]
COMPOSE = ["docker", "compose", "-f", str(ROOT / "infra/local/compose.yaml")]
CURRENT_STAGE = "initializing"


def command(arguments, text=None, timeout=40):
    result = subprocess.run(arguments, input=text, text=True, capture_output=True, timeout=timeout)
    if result.returncode:
        # Do not expose logs or command arguments that could contain credentials.
        raise RuntimeError("Infrastructure command failed; inspect the affected Nexora service locally")
    return result.stdout.strip()


def postgres(sql):
    global CURRENT_STAGE
    CURRENT_STAGE = "PostgreSQL query"
    return command(COMPOSE + ["exec", "-T", "postgres", "sh", "-c",
        'export PGPASSWORD="$(cat /run/secrets/postgres_password)"; '
        'exec psql -h 127.0.0.1 -U nexora -d nexora -Atq -v ON_ERROR_STOP=1'], sql)


def dragonfly(*arguments):
    global CURRENT_STAGE
    CURRENT_STAGE = "Dragonfly " + str(arguments[0])
    flags = (ROOT / ".nexora/local/secrets/dragonfly_flags").read_text().splitlines()
    password = next(line.split("=", 1)[1] for line in flags if line.startswith("--requirepass="))
    port = int(os.environ.get("NEXORA_DRAGONFLY_PORT", "16379"))
    return RedisProbe("127.0.0.1", port, password).command(*arguments)


def graph(statement, run):
    global CURRENT_STAGE
    CURRENT_STAGE = "Neo4j HTTP query"
    username, password = (ROOT / ".nexora/local/secrets/neo4j_auth").read_text().strip().split("/", 1)
    auth = f"{username}:{password}"
    payload = json.dumps({"statements": [{"statement": statement, "parameters": {"run": run}}]}).encode()
    port = int(os.environ.get("NEXORA_NEO4J_HTTP_PORT", "17474"))
    request = urllib.request.Request(f"http://127.0.0.1:{port}/db/neo4j/tx/commit", data=payload,
        headers={"Content-Type": "application/json", "Authorization": "Basic " + base64.b64encode(auth.encode()).decode()})
    with urllib.request.urlopen(request, timeout=30) as response:
        result = json.load(response)
    if result.get("errors"):
        raise RuntimeError("Neo4j rejected infrastructure probe")
    return result["results"][0]["data"]


def check(run, key):
    if postgres(f"SELECT count(*) FROM nexora_infra_probe.markers WHERE id='{run}';") != "1":
        raise RuntimeError("PostgreSQL marker missing")
    if dragonfly("GET", key) != run:
        raise RuntimeError("Dragonfly marker missing")
    rows = graph("MATCH (a:NexoraInfraProbe {run:$run})-[r:NEXORA_CHECK]->(b:NexoraInfraProbe {run:$run}) RETURN count(r)", run)
    if not rows or rows[0]["row"] != [1]:
        raise RuntimeError("Neo4j relationship missing")


def main():
    global CURRENT_STAGE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recreate", action="store_true", help="Recreate only nexora-local containers, preserving volumes")
    args = parser.parse_args()
    run = uuid.uuid4().hex
    key = "nexora:infra-probe:" + run
    postgres("CREATE SCHEMA IF NOT EXISTS nexora_infra_probe; "
        "CREATE TABLE IF NOT EXISTS nexora_infra_probe.markers(id text PRIMARY KEY); "
        f"INSERT INTO nexora_infra_probe.markers VALUES ('{run}');")
    if dragonfly("SET", key, run) != "OK":
        raise RuntimeError("Dragonfly write failed")
    graph("CREATE (a:NexoraInfraProbe {run:$run, position:1})-[:NEXORA_CHECK]->(b:NexoraInfraProbe {run:$run, position:2}) RETURN count(a)", run)
    check(run, key)
    print("PASS: authenticated PostgreSQL read/write, Dragonfly read/write, Neo4j nodes/relationship traversal", flush=True)
    expiry_key = key + ":expiry"
    if dragonfly("SET", expiry_key, "temporary", "PX", "1000") != "OK":
        raise RuntimeError("Dragonfly expiry write failed")
    ttl = int(dragonfly("PTTL", expiry_key))
    if not 0 < ttl <= 1000:
        raise RuntimeError("Dragonfly expiry not applied")
    time.sleep(1.1)
    if dragonfly("EXISTS", expiry_key) != "0":
        raise RuntimeError("Dragonfly key did not expire")
    print("PASS: Dragonfly per-key expiry", flush=True)
    if dragonfly("SAVE") != "OK":
        raise RuntimeError("Dragonfly snapshot failed")
    print("PASS: Dragonfly snapshot completed", flush=True)
    if args.recreate:
        before = {s: command(COMPOSE + ["ps", "-q", s]) for s in ["postgres", "dragonfly", "neo4j"]}
        CURRENT_STAGE = "container recreation and readiness"
        command(COMPOSE + ["up", "-d", "--pull", "never", "--force-recreate", "--wait", "--wait-timeout", "180"], timeout=360)
        after = {s: command(COMPOSE + ["ps", "-q", s]) for s in before}
        if any(not after[s] or before[s] == after[s] for s in before):
            raise RuntimeError("Container recreation was not demonstrated")
        check(run, key)
        print("PASS: all three container identities changed; PostgreSQL, Dragonfly and Neo4j data survived", flush=True)
    postgres(f"DELETE FROM nexora_infra_probe.markers WHERE id='{run}';")
    dragonfly("DEL", key, expiry_key)
    graph("MATCH (n:NexoraInfraProbe {run:$run}) DETACH DELETE n", run)
    dragonfly("SAVE")
    print("PASS: probe records removed; no analyzed repository source was imported", flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"FAIL: {CURRENT_STAGE}: {type(error).__name__}; no credentials printed", file=sys.stderr)
        sys.exit(1)
