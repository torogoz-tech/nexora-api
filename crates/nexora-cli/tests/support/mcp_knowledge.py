"""Public synthetic contribution workflow over actual MCP stdio; no private code."""
import json
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time

class Client:
    def __init__(self, binary, workspace, root, tenant, write=False):
        args = [str(binary), "--connections", str(workspace / ".nexora/local/connections.json"), "--language-dir", str(workspace / "languages/javascript"), "mcp", "serve", "--root", str(root), "--tenant", tenant]
        if write:
            args += ["--allow-index", "--allow-knowledge", "--knowledge-actor", "public-mcp-agent"]
        self.process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.inbox = queue.Queue()
        self.sequence = 0
        self.waiting = {}
        def read():
            for line in self.process.stdout:
                self.inbox.put(json.loads(line))
            self.inbox.put({"closed": True})
        threading.Thread(target=read, daemon=True).start()
        self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "public-knowledge-test", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    def send(self, value):
        self.process.stdin.write(json.dumps(value).encode() + b"\n")
        self.process.stdin.flush()
    def request(self, method, params):
        self.sequence += 1
        request_id = self.sequence
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        while request_id not in self.waiting:
            reply = self.inbox.get(timeout=180)
            assert "closed" not in reply, reply
            self.waiting[reply["id"]] = reply
        return self.waiting.pop(request_id)
    def call(self, name, args=None):
        reply = self.request("tools/call", {"name": name, "arguments": args or {}})
        assert "error" not in reply and not reply["result"].get("isError"), reply
        result = reply["result"]["structuredContent"]
        assert result == json.loads(reply["result"]["content"][0]["text"])
        return result
    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=180) == 0
        self.process.stdout.close()
        self.process.stderr.close()

binary = Path(sys.argv[1]).resolve()
workspace = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="nexora-public-knowledge-") as temp:
    root = Path(temp).resolve()
    (root / "a.js").write_text("function hello() {}\n", encoding="utf-8")
    tenant = "public-mcp-knowledge-" + str(time.time_ns())
    client = Client(binary, workspace, root, tenant, True)
    assert len(client.request("tools/list", {})["result"]["tools"]) == 14
    snapshot = client.call("nexora_index_repository")["snapshot"]
    node = client.call("nexora_find_symbol", {"name": "hello", "snapshot": snapshot})["items"][0]["id"]
    draft = {"kind": "relation", "statement": "The file exposes this function.", "source": "file:a.js", "target": node, "relation": "EXPOSES", "evidence": [{"path": "a.js", "start_line": 1, "line_count": 1}]}
    create = {"request_id": "create-1", "snapshot": snapshot, "draft": draft}
    result = client.call("nexora_contribute_knowledge", create)
    assert result == client.call("nexora_contribute_knowledge", create)
    knowledge_id = result["contribution"]["id"]
    assert result["contribution"]["origin"] == "contributed"
    accepted = client.call("nexora_review_knowledge", {"id": knowledge_id, "request_id": "accept-1", "expected_revision": 1, "action": "accept", "reason": "Source inspected", "snapshot": snapshot})
    assert accepted["contribution"]["effective"]
    assert client.call("nexora_list_knowledge", {"status": "accepted", "node": node})["total"] == 1
    bad = client.request("tools/call", {"name": "nexora_review_knowledge", "arguments": {"id": knowledge_id, "request_id": "stale-1", "expected_revision": 1, "action": "reject", "reason": "Old revision"}})
    assert bad["result"]["isError"] and "expected_revision" in bad["result"]["content"][0]["text"]
    client.call("nexora_list_pending")
    client.close()
    client = Client(binary, workspace, root, tenant)
    assert len(client.request("tools/list", {})["result"]["tools"]) == 10
    detail = client.call("nexora_get_knowledge", {"id": knowledge_id})
    assert detail["history_total"] == 2 and detail["contribution"]["effective"]
    denied = client.request("tools/call", {"name": "nexora_review_knowledge", "arguments": {"id": knowledge_id}})
    assert denied["error"]["code"] == -32602
    client.close()
    client = Client(binary, workspace, root, tenant, True)
    (root / "a.js").write_text("function hello() {}\n// new version\n", encoding="utf-8")
    newer = client.call("nexora_index_repository")["snapshot"]
    assert newer != snapshot
    stale = client.call("nexora_get_knowledge", {"id": knowledge_id})["contribution"]
    assert stale["validity"] == "needs_review" and not stale["effective"]
    assert client.call("nexora_list_knowledge", {"status": "needs_review"})["total"] == 1
    node = client.call("nexora_find_symbol", {"name": "hello"})["items"][0]["id"]
    draft["target"] = node
    revised = client.call("nexora_revise_knowledge", {"id": knowledge_id, "request_id": "revise-1", "expected_revision": 2, "draft": draft})
    assert revised["contribution"]["state"] == "proposed" and revised["contribution"]["revision"] == 3
    rejected = client.call("nexora_review_knowledge", {"id": knowledge_id, "request_id": "reject-1", "expected_revision": 3, "action": "reject", "reason": "Needs another explanation"})
    assert rejected["contribution"]["state"] == "rejected" and not rejected["contribution"]["effective"]
    withdrawn = client.call("nexora_review_knowledge", {"id": knowledge_id, "request_id": "withdraw-1", "expected_revision": 4, "action": "withdraw", "reason": "Replaced"})
    assert withdrawn["contribution"]["state"] == "withdrawn"
    replay = client.call("nexora_review_knowledge", {"id": knowledge_id, "request_id": "accept-1", "expected_revision": 1, "action": "accept", "reason": "Source inspected", "snapshot": snapshot})
    assert not replay["contribution"]["effective"], "An idempotent receipt must not revive a superseded acceptance"
    assert not replay["contribution"]["is_latest"]
    assert client.call("nexora_get_knowledge", {"id": knowledge_id})["history_total"] == 5
    note=client.call("nexora_contribute_knowledge", {"request_id":"explanation-1","draft":{"kind":"explanation","statement":"The file declares the hello entry point.","evidence":[{"path":"a.js","start_line":1,"line_count":1}]}})["contribution"]
    note["draft"]["statement"]="Clarified explanation of the declared hello entry point."
    corrected_note=client.call("nexora_revise_knowledge", {"id":note["id"],"request_id":"explanation-revision-1","expected_revision":1,"draft":note["draft"]})["contribution"]
    assert corrected_note["revision"]==2 and corrected_note["state"]=="proposed"
    client.close()
print(json.dumps({"workflow": "contribute-review-query-restart-invalidate-revise-reject-withdraw", "passed": True, "source": "public generated fixture"}))
