"""Actual local executable lifecycle. Generated credentials never printed."""
import json
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
import urllib.error

binary=Path(sys.argv[1]).resolve()
workspace=Path(__file__).resolve().parents[4]
namespace="nexora_platform_cli_"+str(time.time_ns())
base_args=[str(binary),"--connections",str(workspace/".nexora/local/connections.json"),"--schema",namespace]
client=urllib.request.build_opener(urllib.request.ProxyHandler({}))
with tempfile.TemporaryDirectory(prefix="nexora-api-public-") as folder:
    password=secrets.token_urlsafe(32)
    secret_file=Path(folder)/"owner-password"
    secret_file.write_text(password,encoding="utf-8")
    secret_file.chmod(0o600)
    migration=subprocess.run(base_args+["migrate"],capture_output=True,text=True,timeout=60)
    assert migration.returncode==0,"CLI migration failed"
    bootstrap=base_args+["bootstrap","--organization","Public CLI fixture","--username","owner","--name","Owner","--password-file",str(secret_file)]
    created=subprocess.run(bootstrap,capture_output=True,text=True,timeout=60)
    assert created.returncode==0,"CLI bootstrap failed"
    original=json.loads(created.stdout)
    assert original["owner"]["kind"]=="human"
    assert password not in created.stdout and password not in created.stderr
    again=subprocess.run(bootstrap,capture_output=True,text=True,timeout=60)
    assert again.returncode!=0,"Second organization/bootstrap must fail"
    for _ in range(2):
        server=subprocess.Popen(base_args+["serve","--bind","127.0.0.1:0"],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
        try:
            banner=server.stderr.readline().strip()
            assert banner.startswith("nexora-api listening on 127.0.0.1:"),"Server startup failed"
            address="http://"+banner.split(" on ",1)[1]
            with client.open(address+"/health",timeout=10) as reply:
                assert reply.status==200
                assert json.load(reply)["service"]=="nexora-api"
            request=urllib.request.Request(address+"/api/v1/auth/login",data=json.dumps({"username":"owner","password":password}).encode(),headers={"Content-Type":"application/json"},method="POST")
            try:
                reply=client.open(request,timeout=15)
            except urllib.error.HTTPError as error:
                problem=json.load(error)
                code=problem.get("code")
                safe_code=code if code in {"STORAGE_UNAVAILABLE","CAPACITY_UNAVAILABLE","UNAUTHORIZED","FORBIDDEN","INVALID_INPUT","CONFLICT"} else "UNKNOWN_ERROR"
                raise AssertionError(f"CLI login HTTP{error.code} {safe_code}") from None
            with reply:
                session=json.load(reply)
                assert session["actor_id"]==original["owner"]["id"]
                assert session["token_type"]=="Bearer"
            assert server.poll() is None,"Server exited during normal requests"
        finally:
            server.send_signal(signal.SIGINT)
            code=server.wait(timeout=30)
            diagnostics=server.stderr.read()
            server.stderr.close()
            assert code==0,"Graceful server shutdown failed"
            assert "panicked" not in diagnostics and password not in diagnostics
print(json.dumps({"cli_lifecycle":"migrate-bootstrap-start-login-stop-restart", "passed":True,"private_project_sources_used":False}))
