# /// script
# dependencies = ["neo4j>=5,<6"]
# ///
"""Check host-advertised Neo4j endpoints and authenticated direct/routing Bolt access."""
from pathlib import Path
import json
import logging
import os
import sys
import urllib.request
from neo4j import GraphDatabase


def main():
    logging.getLogger("neo4j").setLevel(logging.CRITICAL)
    root = Path(__file__).resolve().parents[2]
    username, password = (root / ".nexora/local/secrets/neo4j_auth").read_text().strip().split("/", 1)
    http_port = int(os.environ.get("NEXORA_NEO4J_HTTP_PORT", "17474"))
    bolt_port = int(os.environ.get("NEXORA_NEO4J_BOLT_PORT", "17687"))
    host = os.environ.get("NEXORA_NEO4J_ADVERTISED_HOST", "127.0.0.1")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(f"http://127.0.0.1:{http_port}/", timeout=15) as response:
        metadata = json.load(response)
    for field, scheme in [("bolt_direct", "bolt"), ("bolt_routing", "neo4j")]:
        if metadata.get(field) != f"{scheme}://{host}:{bolt_port}":
            raise RuntimeError("Server advertises a different Bolt endpoint than the published host port")
    print("PASS: direct and routing discovery advertise the published host port", flush=True)
    for scheme in ["bolt", "neo4j"]:
        with GraphDatabase.driver(f"{scheme}://127.0.0.1:{bolt_port}", auth=(username, password),
                connection_timeout=5, connection_acquisition_timeout=15, max_transaction_retry_time=0) as driver:
            driver.verify_connectivity()
            with driver.session(database="neo4j") as session:
                if session.run("RETURN 1 AS value").single()["value"] != 1:
                    raise RuntimeError("Bolt query failed")
            with driver.session(database="system") as session:
                if not list(session.run("SHOW DATABASES")):
                    raise RuntimeError("System database discovery failed")
        print(f"PASS: {scheme} connection, query and system database discovery", flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"FAIL: {type(error).__name__}; {getattr(error, 'code', '')}; credentials omitted", file=sys.stderr)
        sys.exit(1)
