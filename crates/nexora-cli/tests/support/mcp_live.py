"""Public synthetic end-to-end MCP test; never imports private application source."""
import json
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time

class Client:
    def __init__(self, binary, workspace, root, tenant, indexing=False, version="2025-11-25"):
        args = [str(binary), "--connections", str(workspace / ".nexora/local/connections.json"), "--language-dir", str(workspace / "languages/javascript") if indexing else str(root / "missing-molds"), "mcp", "serve", "--root", str(root), "--tenant", tenant]
        if indexing:
            args.append("--allow-index")
        self.process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.inbox = queue.Queue()
        self.sequence = 0
        self.pending = {}
        def read():
            for line in self.process.stdout:
                try:
                    self.inbox.put(json.loads(line))
                except ValueError:
                    self.inbox.put({"invalid_output": True})
            self.inbox.put({"closed": True})
        threading.Thread(target=read, daemon=True).start()
        reply = self.request("initialize", {"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "nexora-public-test", "version": "1"}})
        assert reply["result"]["protocolVersion"] == version, reply
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    def send(self, value):
        self.process.stdin.write(json.dumps(value).encode() + b"\n")
        self.process.stdin.flush()
    def response(self, request_id, timeout=180):
        key = json.dumps(request_id)
        if key in self.pending:
            return self.pending.pop(key)
        deadline = time.monotonic() + timeout
        while True:
            reply = self.inbox.get(timeout=max(0.01, deadline - time.monotonic()))
            assert "closed" not in reply and "invalid_output" not in reply, reply
            if reply.get("id") == request_id and type(reply.get("id")) is type(request_id):
                return reply
            self.pending[json.dumps(reply.get("id"))] = reply
    def request(self, method, params):
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        return self.response(self.sequence)
    def call(self, name, args=None):
        reply = self.request("tools/call", {"name": name, "arguments": args or {}})
        assert "error" not in reply, reply
        assert not reply["result"].get("isError"), reply
        result = reply["result"].get("structuredContent") or json.loads(reply["result"]["content"][0]["text"])
        assert result == json.loads(reply["result"]["content"][0]["text"])
        return result
    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=180) == 0
        self.process.stdout.close()
        self.process.stderr.close()

binary = Path(sys.argv[1]).resolve()
workspace = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="nexora-mcp-public-") as temp:
    root = Path(temp).resolve()
    (root / "a.js").write_text("function hello() {}\n", encoding="utf-8")
    (root / "b.js").write_text("class World {}\n", encoding="utf-8")
    tenant = "public-mcp-" + str(time.time_ns())
    client = Client(binary, workspace, root, tenant, indexing=True)
    assert len(client.request("tools/list", {})["result"]["tools"]) == 11
    first = client.call("nexora_index_repository")
    assert first["reuse"]["analyzed_files"] == 2, first
    pinned = {"snapshot": first["snapshot"]}
    symbol = client.call("nexora_find_symbol", dict(pinned, name="hello"))["items"][0]
    assert symbol["evidence"]["path"] == "a.js"
    traversed = client.call("nexora_traverse_graph", dict(pinned, node="file:a.js", depth=3))
    assert symbol["id"] in [n["id"] for n in traversed["nodes"]]
    context = client.call("nexora_get_source_context", dict(pinned, path="a.js"))
    assert context["items"][0]["text"] == "function hello() {}"
    warm = client.call("nexora_index_repository")
    assert warm["snapshot"] == first["snapshot"]
    assert warm["reuse"]["reused_files"] == 2 and warm["reuse"]["analyzed_files"] == 0
    client.close()
    # Restart with missing mold directory and deleted live source: persisted reads must still work.
    (root / "a.js").unlink()
    client = Client(binary, workspace, root, tenant)
    assert len(client.request("tools/list", {})["result"]["tools"]) == 10
    restored = client.call("nexora_get_source_context", dict(pinned, path="a.js"))
    assert restored == context
    assert client.call("nexora_find_symbol", dict(pinned, name="hello"))["items"][0] == symbol
    denial = client.request("tools/call", {"name": "nexora_repository_overview", "arguments": {"tenant": "other"}})
    assert denial["error"]["code"] == -32602
    client.close()
    (root / "a.js").write_text("function changed() {}\n", encoding="utf-8")
    client = Client(binary, workspace, root, tenant, indexing=True)
    changed = client.call("nexora_index_repository")
    assert changed["snapshot"] != first["snapshot"]
    assert changed["reuse"]["reused_files"] == 1 and changed["reuse"]["analyzed_files"] == 1
    assert client.call("nexora_find_symbol", dict(pinned, name="hello"))["items"][0] == symbol
    assert client.call("nexora_find_symbol", {"name": "changed"})["items"]
    client.close()
    # A different authorized tenant cannot retrieve the previous snapshot, even knowing its ID.
    client = Client(binary, workspace, root, tenant + "-other")
    absent = client.request("tools/call", {"name": "nexora_repository_overview", "arguments": pinned})
    assert absent["result"]["isError"] is True
    assert str(workspace) not in json.dumps(absent)
    client.close()
print(json.dumps({"live_mcp": "passed", "files": 2, "restart_without_molds_or_live_source": True, "warm_reused": 2, "changed_reused": 1, "changed_analyzed": 1, "neo4j_traversal": True, "tenant_isolation": True}))
