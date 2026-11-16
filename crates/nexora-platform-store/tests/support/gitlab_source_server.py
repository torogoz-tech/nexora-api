"""Generated-repository fixture: GitLab metadata and authenticated smart HTTP.

Arguments contain paths and a public commit ID only. Credentials are generated here
and returned solely through a mode-0600 settings file inside the private fixture.
"""
import base64
import hmac
import http.server
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import threading
import time
import urllib.parse


class GitlabSourceServer(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send_body(self, status, body, content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def refresh_tokens(self):
        control = self.server.fixture["control"]
        counts = control.with_suffix(".refresh-count")
        count = int(counts.read_text()) if counts.exists() else 0
        counts.write_text(str(count + 1))
        if count:
            self.send_body(401, b'{"error":"refresh token consumed"}')
            return
        length = int(self.headers.get("Content-Length", "0"))
        form = urllib.parse.parse_qs(self.rfile.read(length).decode())
        if form.get("grant_type") != ["refresh_token"]:
            self.send_body(400, b"{}")
            return
        self.server.fixture["token"] = secrets.token_urlsafe(32)
        value = {"access_token": self.server.fixture["token"],
                 "refresh_token": secrets.token_urlsafe(32), "token_type": "Bearer",
                 "expires_in": 3600, "refresh_token_expires_in": 86400,
                 "scope": "" if self.path == "/login/oauth/access_token" else "read_api read_repository"}
        self.send_body(200, json.dumps(value).encode())

    def authorized(self, git):
        token = self.server.fixture["token"]
        expected = (
            "Basic " + base64.b64encode(("oauth2:" + token).encode()).decode()
            if git else "Bearer " + token
        )
        if hmac.compare_digest(self.headers.get("Authorization", ""), expected):
            return True
        self.send_response(401)
        if git:
            self.send_header("WWW-Authenticate", 'Basic realm="generated-fixture"')
        self.send_header("Content-Length", "0")
        self.end_headers()
        return False

    def handle_request(self):
        uri = urllib.parse.urlsplit(self.path)
        path = urllib.parse.unquote(uri.path)
        git = path.startswith("/fixture/source.git/")
        if self.command == "POST" and path == "/oauth/token":
            return self.refresh_tokens()
        if not self.authorized(git):
            return
        control = self.server.fixture["control"]
        mode = json.loads(control.read_text()) if control.exists() else {}
        if path.startswith("/api/v4/projects/") and mode.get("deny_repository"):
            self.send_body(403, b'{"message":"generated repository denial"}')
            return
        if path.startswith("/api/v4/projects/") and mode.get("rate_limit"):
            self.send_body(429, b'{"message":"generated temporary rate limit"}')
            return
        if git:
            self.serve_git(uri)
            return
        if mode.get("workflow") and self.command == "POST" and path == "/api/v4/projects/101/repository/branches":
            length = int(self.headers.get("Content-Length", "0"))
            data = json.loads(self.rfile.read(length))
            branches = self.server.fixture.setdefault("created_branches", {})
            if data["branch"] in branches or data["branch"] in mode.get("branches", {}):
                self.send_body(400, b'{"message":"Branch already exists"}')
                return
            assert len(data["ref"]) in (40, 64), "Branch creation did not pin a commit SHA"
            branches[data["branch"]] = data["ref"]
            control.with_suffix(".branch-count").write_text(str(len(branches)))
            if mode.get("pause_branch_creation"):
                expires = time.monotonic() + 8
                while not control.with_suffix(".release-branch").exists() and time.monotonic() < expires:
                    time.sleep(0.01)
            if mode.get("branch_lost_response"):
                self.close_connection = True
                return
            self.send_body(201, json.dumps({"name":data["branch"],"commit":{"id":data["ref"]}}).encode())
            return
        if mode.get("workflow") and self.command == "GET" and path == "/api/v4/projects/101/merge_requests" and "merged_requests" in mode:
            self.send_body(200, json.dumps(mode["merged_requests"]).encode())
            return
        if mode.get("workflow") and self.command == "GET" and path == "/api/v4/projects/101/deployments":
            self.send_body(200, json.dumps(mode.get("deployments", [])).encode())
            return
        if mode.get("workflow") and path == "/api/v4/projects/101/merge_requests":
            requests = self.server.fixture.setdefault("workflow_prs", [])
            if self.command == "POST":
                length = int(self.headers.get("Content-Length", "0"))
                data = json.loads(self.rfile.read(length))
                value = {"iid": len(requests) + 1, "title": data["title"], "description": data["description"],
                         "source_branch": data["source_branch"], "target_branch": data["target_branch"],
                         "sha": mode["branches"][data["source_branch"]], "state": "opened", "diff_refs": {"base_sha": "b" * 40}}
                requests.append(value)
                control.with_suffix(".pr-count").write_text(str(len(requests)))
                if mode.get("pr_lost_response"):
                    self.close_connection = True
                    return
                self.send_body(201, json.dumps(value).encode())
            else:
                values = [dict(value, sha=mode["branches"][value["source_branch"]]) for value in requests]
                self.send_body(200, json.dumps(values).encode())
            return
        if mode.get("workflow") and path.startswith("/api/v4/projects/101/repository/branches/"):
            branch = path.removeprefix("/api/v4/projects/101/repository/branches/")
            branches = dict(self.server.fixture.get("created_branches", {}), **mode.get("branches", {}))
            if branch in self.server.fixture.get("created_branches", {}) and mode.get("branch_read_lost_once") and not self.server.fixture.get("branch_read_failed"):
                self.server.fixture["branch_read_failed"] = True
                self.send_body(503, b'{}')
                return
            if branch not in branches:
                self.send_body(404, b"{}")
                return
            self.send_body(200, json.dumps({"name": branch, "commit": {"id": branches[branch]}}).encode())
            return
        if mode.get("workflow") and path == "/api/v4/projects/101/repository/merge_base":
            self.send_body(200, json.dumps({"id": mode.get("merge_base", mode["branches"]["develop"])}).encode())
            return
        if self.command != "GET":
            self.send_body(405, b"{}")
            return
        commit = self.server.fixture["commit"]
        if path == "/api/v4/user":
            if mode.get("identity_failure"):
                self.send_body(503, b"{}")
                return
            value = {"id": 43 if mode.get("identity_mismatch") else 42, "username": "fixture"}
        elif path == "/api/v4/projects":
            value = []
        elif path in ("/api/v4/projects/101", "/api/v4/projects/fixture/source"):
            value = {
                "id": 101, "path_with_namespace": "fixture/source",
                "repository_access_level": "enabled", "default_branch": "main",
            }
        elif path == "/api/v4/projects/101/merge_requests/7":
            value = {"iid": 7, "title": "Fixture PR", "sha": mode.get("pr_head", "a" * 40),
                     "diff_refs": {"base_sha": "b" * 40}, "state": "opened", "source_branch": "feature", "target_branch": "main"}
        elif path == "/api/v4/projects/101/merge_requests/7/diffs":
            value = [{"new_path": "source.rs", "old_path": "source.rs", "diff": "@@ -1 +1 @@\n-old\n+new", "too_large": bool(mode.get("partial_diff"))}]
        elif path == "/api/v4/projects/101/repository/branches":
            value = [{"name": "main", "commit": {"id": commit}}]
        elif path == "/api/v4/projects/101/repository/commits":
            value = [{"id": "invalid" if mode.get("invalid_commit") else commit}]
        elif path.startswith("/api/v4/projects/101/repository/commits/") and path.rsplit("/", 1)[-1] in set(mode.get("branches", {}).values()) | set(self.server.fixture.get("created_branches", {}).values()) | {commit}:
            value = {"id": path.rsplit("/", 1)[-1]}
        else:
            self.send_body(404, b"{}")
            return
        self.send_body(200, json.dumps(value).encode())

    def serve_git(self, uri):
        control = self.server.fixture["control"]
        pause = control.with_suffix(".git-pause")
        if pause.exists():
            control.with_suffix(".git-waiting").write_bytes(b"")
            deadline = time.monotonic() + 60
            while pause.exists():
                if time.monotonic() >= deadline:
                    self.send_body(504, b"{}")
                    return
                time.sleep(0.01)
        length = int(self.headers.get("Content-Length", "0"))
        if length < 0 or length > 1_048_576:
            self.send_body(413, b"{}")
            return
        environment = {
            "LC_ALL": "C", "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null",
            "GIT_PROJECT_ROOT": str(self.server.fixture["root"]),
            "GIT_HTTP_EXPORT_ALL": "1", "REMOTE_USER": "generated-fixture",
            "REQUEST_METHOD": self.command, "PATH_INFO": uri.path,
            "QUERY_STRING": uri.query, "CONTENT_TYPE": self.headers.get("Content-Type", ""),
            "CONTENT_LENGTH": str(length), "SERVER_PROTOCOL": "HTTP/1.1",
            "HTTP_GIT_PROTOCOL": self.headers.get("Git-Protocol", ""),
        }
        try:
            result = subprocess.run(
                ["/usr/bin/git", "http-backend"], env=environment,
                input=self.rfile.read(length), stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL, timeout=20, check=False,
            )
            headers, body = result.stdout.split(b"\r\n\r\n", 1)
        except (OSError, ValueError, subprocess.TimeoutExpired):
            self.send_body(502, b"{}")
            return
        status = 200
        forwarded = []
        for line in headers.split(b"\r\n"):
            name, value = line.decode("latin1").split(":", 1)
            if name.lower() == "status":
                status = int(value.strip().split(" ", 1)[0])
            elif name.lower() not in ("content-length", "connection", "transfer-encoding"):
                forwarded.append((name, value.strip()))
        self.send_response(status)
        for name, value in forwarded:
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = handle_request
    do_POST = handle_request


if __name__ == "__main__":
    root, settings, control = map(Path, sys.argv[1:4])
    commit = sys.argv[4]
    if len(commit) not in (40, 64) or any(c not in "0123456789abcdef" for c in commit):
        raise SystemExit("invalid generated commit identifier")
    token = secrets.token_urlsafe(32)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), GitlabSourceServer)
    server.daemon_threads = True
    server.fixture = {"token": token, "root": root, "commit": commit, "control": control}
    descriptor = os.open(settings, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as stream:
        json.dump({"origin": "http://127.0.0.1:" + str(server.server_port), "token": token}, stream)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    print("ready", flush=True)
    sys.stdin.buffer.read(1)
    server.shutdown()
    server.server_close()
    thread.join()
