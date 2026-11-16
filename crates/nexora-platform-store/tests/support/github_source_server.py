# /// script
# requires-python = ">=3.11"
# dependencies = ["cryptography==46.0.3"]
# ///
"""Generated GitHub App fixture with signed JWT and repository-scoped Git transport."""
import base64
import hmac
import http.server
import json
import os
from pathlib import Path
import secrets
import sys
import threading
import time
import urllib.parse
from cryptography.hazmat.primitives.asymmetric import rsa, padding
from cryptography.hazmat.primitives import serialization, hashes
from gitlab_source_server import GitlabSourceServer


class GithubSourceServer(GitlabSourceServer):
    def do_PATCH(self):
        self.handle_request()

    def app(self):
        try:
            token = self.headers.get("Authorization", "").removeprefix("Bearer ")
            header, payload, signature = token.split(".")
            metadata = json.loads(base64.urlsafe_b64decode(header + "==="))
            claims = json.loads(base64.urlsafe_b64decode(payload + "==="))
            assert metadata["alg"] == "RS256" and claims["iss"] == "fixture-client"
            assert time.time() - 100 < claims["iat"] <= time.time()
            assert time.time() < claims["exp"] <= time.time() + 600
            self.server.fixture["key"].public_key().verify(
                base64.urlsafe_b64decode(signature + "==="),
                (header + "." + payload).encode(), padding.PKCS1v15(), hashes.SHA256(),
            )
            return True
        except Exception:
            return False

    def user(self):
        return hmac.compare_digest(
            self.headers.get("Authorization", ""), "Bearer " + self.server.fixture["token"]
        )

    def scoped(self):
        return self.headers.get("Authorization", "").removeprefix("Bearer ") in self.server.fixture["scoped"]

    def authorized(self, git):
        if not git:
            return self.user() or self.scoped()
        try:
            scheme, encoded = self.headers.get("Authorization", "").split(" ", 1)
            username, token = base64.b64decode(encoded, validate=True).decode().split(":", 1)
            valid = scheme == "Basic" and username == "x-access-token" and token in self.server.fixture["scoped"]
        except (ValueError, UnicodeError):
            valid = False
        if valid:
            return True
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="generated-fixture"')
        self.send_header("Content-Length", "0")
        self.end_headers()
        return False

    def handle_request(self):
        uri = urllib.parse.urlsplit(self.path)
        path = urllib.parse.unquote(uri.path)
        if path.startswith("/fixture/source.git/"):
            if self.authorized(True):
                self.serve_git(uri)
            return
        mode_path = self.server.fixture["control"]
        mode = json.loads(mode_path.read_text()) if mode_path.exists() else {}
        commit = self.server.fixture["commit"]
        if mode.get("failure_path") == path:
            self.send_body(int(mode["failure_status"]), b'{"message":"provider access failed"}')
            return
        permissions = {"contents": "read", "metadata": "read"}
        if self.command == "POST" and path == "/login/oauth/access_token":
            return self.refresh_tokens()
        if self.command == "POST" and path == "/app/installations/7/access_tokens":
            if not self.app():
                self.send_body(401, b"{}")
                return
            length = int(self.headers.get("Content-Length", "0"))
            if length < 0 or length > 65536:
                self.send_body(413, b"{}")
                return
            value = json.loads(self.rfile.read(length))
            if value.get("repository_ids") != [101] or value.get("permissions") != permissions:
                self.send_body(403, b'{"message":"excessive generated permissions"}')
                return
            token = secrets.token_urlsafe(32)
            self.server.fixture["scoped"].add(token)
            result = {"token": token, "expires_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time()+3600)),
                      "permissions": permissions, "repositories": [{"id": 101, "full_name": "fixture/source"}]}
            self.send_body(201, json.dumps(result).encode())
            return
        if self.command == "PATCH" and path == "/app/hook/config" and self.app():
            if mode.get("hook_failure"):
                self.send_body(503,b"{}");return
            value=json.loads(self.rfile.read(int(self.headers.get("Content-Length","0"))))
            assert value["content_type"]=="json" and value["insecure_ssl"]=="0" and len(value["secret"])>=32
            value["secret"]="********"
            self.send_body(200,json.dumps(value).encode());return
        if self.command != "GET":
            self.send_body(405, b"{}")
            return
        if path == "/app" and self.app():
            result = {"id": 88, "slug": "nexora-fixture", "permissions": permissions, "events":([] if mode.get("no_push") else ["push","repository"])}
        elif path == "/app/installations/7" and self.app():
            result = {"id": 7, "app_id": 88, "suspended_at": None, "permissions": permissions,
                      "repository_selection": "selected", "account": {"id": 707, "login": "fixture-org"}}
        elif path == "/user" and self.user():
            if mode.get("identity_failure"):
                self.send_body(503, b"{}")
                return
            result = {"id": 43 if mode.get("identity_mismatch") else 42, "login": "fixture"}
        elif path == "/user/installations/7/repositories" and self.user():
            result = {"total_count": 1, "repositories": [{"id": 101, "full_name": "fixture/source"}]}
        elif path.startswith("/repos/fixture/source") and self.authorized(False):
            if mode.get("rate_limit"):
                body = b'{"message":"secondary rate limit"}'
                self.send_response(403)
                self.send_header("X-RateLimit-Remaining", "0")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            if mode.get("deny_repository"):
                self.send_body(403, b'{"message":"Resource not accessible by integration"}')
                return
            if path == "/repos/fixture/source":
                result = {"id": 101, "full_name": "fixture/source", "disabled": False}
            elif path == "/repos/fixture/source/branches":
                result = [{"name": "main", "commit": {"sha": commit}}]
                if mode.get("branches"): result.append({"name":"feature/ui","commit":{"sha":"d"*40}})
            elif path == "/repos/fixture/source/commits" and self.scoped():
                query=urllib.parse.parse_qs(uri.query)
                page=int(query.get("page",["1"])[0]); size=int(query.get("per_page",["1"])[0])
                if size==20 and mode.get("fail_page_two") and page==2:
                    self.send_body(503,b"{}");return
                count=21 if size==20 else (2 if mode.get("branches") else 1)
                result=[self.commit_record(commit if i==0 else format(i,"040x")) for i in range((page-1)*size,min(page*size,count))]
                if size==100 and mode.get("branches") and query.get("sha")==["d"*40]: result=[self.commit_record(commit),self.commit_record("d"*40)] if page==1 else []
                if size==1 and mode.get("invalid_commit") and result: result[0]["sha"]="invalid"
            elif path.startswith("/repos/fixture/source/compare/") and self.scoped():
                if mode.get("compare_missing"):
                    self.send_body(404,b"{}");return
                result={"status":"ahead","commits":[self.commit_record(format(30,"040x"))]}
            elif path == "/repos/fixture/source/commits/" + commit and self.scoped():
                result = self.commit_record(commit)
                result["files"]=[{"filename":"src/example.rs","status":"modified","additions":1,"deletions":0,"patch":"@@ -1 +1 @@\n-old\n+new"}]
            else:
                self.send_body(404, b"{}")
                return
        else:
            self.send_body(401, b"{}")
            return
        self.send_body(200, json.dumps(result).encode())

    def commit_record(self,sha):
        return {"sha":sha,"commit":{"message":"Commit "+sha,"author":{"name":"Fixture Author","email":"fixture@example.test","date":"2026-10-01T12:00:00Z"},"committer":{"date":"2026-10-01T12:00:00Z"}}}

    do_GET = handle_request
    do_POST = handle_request


if __name__ == "__main__":
    root, settings, control = map(Path, sys.argv[1:4])
    commit = sys.argv[4]
    if len(commit) not in (40, 64) or any(c not in "0123456789abcdef" for c in commit):
        raise SystemExit("invalid generated commit identifier")
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    pem = key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()).decode()
    token = secrets.token_urlsafe(32)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), GithubSourceServer)
    server.daemon_threads = True
    server.fixture = {"token": token, "key": key, "root": root, "commit": commit, "control": control, "scoped": set()}
    descriptor = os.open(settings, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as stream:
        json.dump({"origin": "http://127.0.0.1:" + str(server.server_port), "token": token, "pem": pem,
                   "client_secret": secrets.token_urlsafe(32), "webhook_secret": secrets.token_urlsafe(32)}, stream)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    print("ready", flush=True)
    sys.stdin.buffer.read(1)
    server.shutdown()
    server.server_close()
    thread.join()
