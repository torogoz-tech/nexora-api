# /// script
# dependencies = ["cryptography==46.0.3"]
# ///
"""Corporate identity fixture. Ephemeral keys/tokens and private requests are never logged."""
import base64, hashlib, json, secrets, signal, socket, subprocess, sys, tempfile, threading, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlencode, urlsplit
import urllib.request, urllib.error
from cryptography.hazmat.primitives.asymmetric import rsa, padding
from cryptography.hazmat.primitives import hashes

b64=lambda b:base64.urlsafe_b64encode(b).rstrip(b"=").decode()
key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
pub=key.public_key().public_numbers()
client_secret=secrets.token_urlsafe(32)
codes={}
mode="valid"
subject="corporate-person"

class Provider(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def send_json(self,value,status=200):
        data=json.dumps(value).encode();self.send_response(status);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
    def do_GET(self):
        path=urlsplit(self.path).path
        if path.endswith("/.well-known/openid-configuration"):
            self.send_json({"issuer":issuer,"authorization_endpoint":origin+"/authorize","token_endpoint":origin+"/token","jwks_uri":origin+"/jwks","response_types_supported":["code"],"subject_types_supported":["public"],"id_token_signing_alg_values_supported":["RS256"],"token_endpoint_auth_methods_supported":["client_secret_post"]})
        elif path=="/jwks":
            self.send_json({"keys":[{"kty":"RSA","kid":"fixture-1","use":"sig","alg":"RS256","n":b64(pub.n.to_bytes((pub.n.bit_length()+7)//8,"big")),"e":b64(pub.e.to_bytes((pub.e.bit_length()+7)//8,"big"))}]})
        else:self.send_json({},404)
    def do_POST(self):
        if self.path!="/token":self.send_json({},404);return
        data=parse_qs(self.rfile.read(int(self.headers["Content-Length"])).decode())
        code=data.get("code",[""])[0];entry=codes.pop(code,None)
        if not entry or b64(hashlib.sha256(data.get("code_verifier",[""])[0].encode()).digest())!=entry["code_challenge"] or data.get("client_secret",[""])[0]!=client_secret:
            self.send_json({"error":"invalid_grant"},400);return
        access_token=secrets.token_urlsafe(32)
        now=int(time.time());claims={"iss":issuer,"sub":subject,"aud":"fixture-client","exp":now+300,"iat":now,"nonce":entry["nonce"],"email":"same-email@example.invalid","roles":["installation-admin"],"at_hash":b64(hashlib.sha256(access_token.encode()).digest()[:16])}
        if mode=="email_verified":claims["email_verified"]=True;claims["email"]="verified@example.invalid"
        if mode=="preferred_contact":claims.pop("email",None);claims["preferred_username"]="preferred@example.invalid"
        if mode=="issuer":claims["iss"]=origin+"/other-tenant"
        if mode=="audience":claims["aud"]="another-client"
        if mode=="nonce":claims["nonce"]="wrong-nonce"
        if mode=="expired":claims["exp"]=now-120
        if mode=="access_hash":claims["at_hash"]="invalid-token-hash"
        if mode=="future_iat":claims["iat"]=now+300
        header=b64(json.dumps({"alg":"RS256","kid":"fixture-1","typ":"JWT"},separators=(",",":")).encode());payload=b64(json.dumps(claims,separators=(",",":")).encode());message=(header+"."+payload).encode()
        signature=key.sign(message,padding.PKCS1v15(),hashes.SHA256())
        if mode=="signature":signature=bytes([signature[0]^1])+signature[1:]
        self.send_json({"access_token":access_token,"token_type":"Bearer","expires_in":300,"id_token":header+"."+payload+"."+b64(signature)})

def main():
    global issuer,origin,mode,subject
    binary=str(Path(sys.argv[1]).resolve());workspace=Path(__file__).resolve().parents[4]
    provider=ThreadingHTTPServer(("127.0.0.1",0),Provider);threading.Thread(target=provider.serve_forever,daemon=True).start();origin=f"http://127.0.0.1:{provider.server_port}";issuer=origin+"/tenant"
    with socket.socket() as listener:listener.bind(("127.0.0.1",0));api_port=listener.getsockname()[1]
    base=f"http://127.0.0.1:{api_port}";namespace="nexora_platform_oidc_"+str(time.time_ns())
    args=[binary,"--connections",str(workspace/".nexora/local/connections.json"),"--schema",namespace]
    http=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def request(path,body=None,token=None,cookie=None,method=None,idempotency_key=None,browser=False):
        headers={"Content-Type":"application/json"}
        if method=="PATCH" and not idempotency_key:idempotency_key=secrets.token_urlsafe(24)
        if idempotency_key:headers["Idempotency-Key"]=idempotency_key
        if browser:headers["X-Nexora-Browser"]=base;headers["Origin"]=base
        if token:headers["Authorization"]="Bearer "+token
        if cookie:headers["Cookie"]=cookie
        req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method or ("POST" if body is not None else "GET"))
        try:reply=http.open(req,timeout=30)
        except urllib.error.HTTPError as error:reply=error
        raw=reply.read();return reply.status,json.loads(raw) if raw else None,reply.headers
    with tempfile.TemporaryDirectory(prefix="nexora-corporate-oidc-") as folder:
        password=secrets.token_urlsafe(32);password_file=Path(folder)/"password";password_file.write_text(password);password_file.chmod(0o600)
        for command in [["migrate"],["bootstrap","--organization","Corporate fixture","--username","owner","--name","Owner","--password-file",str(password_file)]]:
            result=subprocess.run(args+command,capture_output=True,text=True,timeout=60);assert result.returncode==0,"OIDC fixture bootstrap failed"
        # Configure through the API first. A pre-feature executable must fail on the missing route.
        server=subprocess.Popen(args+["serve","--bind",f"127.0.0.1:{api_port}"],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
        try:
            assert server.stderr.readline().startswith("nexora-api listening"),"OIDC fixture server failed"
            status,session,_=request("/api/v1/auth/login",{"username":"owner","password":password});assert status==200,"Local bootstrap login failed"
            assert session["next_step"]=="totp_enroll"
            assert request("/api/v1/projects",token=session["access_token"])[0]==401,"First factor accessed API"
            status,session,_=request("/api/v1/auth/gate",{"action":"skip"},session["access_token"]);assert status==200
            owner=session["access_token"];owner_id=session["actor_id"]
            status,_,_=request("/api/v1/auth/oidc/providers",token=owner);assert status==200,"OIDC provider API missing"
        finally:
            server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,"OIDC server stop failed";server.stderr.close()
        policy_file=Path(folder)/"identity-policy.json";policy_file.write_text(json.dumps({"allowed_provider_origins":[origin],"allowed_callback_origins":[base]}))
        server=subprocess.Popen(args+["serve","--bind",f"127.0.0.1:{api_port}","--identity-policy",str(policy_file)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
        try:
            assert server.stderr.readline().startswith("nexora-api listening"),"OIDC policy startup failed"
            status,provider_meta,_=request("/api/v1/auth/oidc/providers",{"name":"Corporate directory","issuer":issuer,"client_id":"fixture-client","client_secret":client_secret,"redirect_uri":base+"/api/v1/auth/oidc/callback","enabled":True,"expected_revision":0},owner);assert status==201,"OIDC configuration failed";provider_id=provider_meta["id"]
            assert request("/api/v1/auth/methods")[0]==200,"Authentication options must be public"
            denied_config={"name":"Untrusted directory","issuer":"https://unconfigured.invalid/tenant","client_id":"fixture-client","client_secret":client_secret,"redirect_uri":base+"/api/v1/auth/oidc/callback","enabled":True,"expected_revision":0}
            assert request("/api/v1/auth/oidc/providers",denied_config,owner)[0]==422,"Client bypassed operator issuer allowlist"
            status,listing,_=request("/api/v1/auth/oidc/providers",token=owner);assert status==200 and client_secret not in json.dumps(listing),"OIDC configuration leaked secret"
            assert listing["allowed_callback_origins"]==[base],"Missing OIDC callback policy metadata"
            provider_path=f"/api/v1/auth/oidc/providers/{provider_id}"
            patch={"name":"Renamed corporate directory","expected_revision":provider_meta["revision"]}
            receipt=secrets.token_urlsafe(24)
            status,provider_meta,_=request(provider_path,patch,owner,method="PATCH",idempotency_key=receipt)
            assert status==200 and provider_meta["name"]==patch["name"] and "client_secret" not in provider_meta,"OIDC metadata PATCH failed or leaked secret"
            assert request(provider_path,patch,owner,method="PATCH",idempotency_key=receipt)[1]==provider_meta,"PATCH retry did not replay"
            assert request(provider_path,patch,owner,method="PATCH")[0]==409,"Stale PATCH accepted"
            assert request(provider_path,{"client_id":"another-client","expected_revision":provider_meta["revision"]},owner,method="PATCH")[0]==422,"Client change accepted without fresh secret"
            assert request(provider_path,{"issuer":issuer,"expected_revision":provider_meta["revision"]},owner,method="PATCH")[0]==422,"Issuer mutation accepted"
            assert request(provider_path,{"name":"Anonymous","expected_revision":provider_meta["revision"]},method="PATCH")[0]==401,"Anonymous provider mutation accepted"
            status,person,_=request("/api/v1/users/oidc",{"username":"corporate-person","name":"Corporate Person","provider_id":provider_id,"subject":"corporate-person"},owner);assert status==201,"OAuth-only user creation failed"
            status,manager,_=request("/api/v1/actors",{"kind":"agent","name":"Limited identity manager","purpose":"No administrator impersonation"},owner);assert status==201,"Identity manager fixture failed"
            status,_,_=request(f"/api/v1/actors/{manager['id']}/grants",{"capability":"actors:manage"},owner);assert status==204,"Identity manager grant failed"
            status,manager_key,_=request(f"/api/v1/actors/{manager['id']}/api-keys",{"name":"Manager credential","scope":{"capabilities":["actors:manage"],"project_ids":[],"all_projects":False,"installation":True},"expires_in_seconds":3600},owner);assert status==201,"Identity manager credential failed"
            assert request(f"/api/v1/actors/{owner_id}/identities/oidc",{"provider_id":provider_id,"subject":"untrusted-manager"},manager_key["key"])[0]==403,"Identity mapping escalated beyond the caller grants"
            status,_,_=request(f"/api/v1/actors/{owner_id}/identities/oidc",{"provider_id":provider_id,"subject":"corporate-owner"},owner);assert status==204,"Owner OIDC mapping failed"
            status,directory,_=request("/api/v1/settings/members",token=owner)
            assert status==200,"Member profile catalog failed"
            corporate=next(item for item in directory if item["id"]==person["id"])
            native=next(item for item in directory if item["id"]==owner_id)
            assert corporate["has_password"] is False and len(corporate["oidc_providers"])==1,"Corporate-only member authentication misidentified"
            assert native["has_password"] is True and len(native["oidc_providers"])==1,"Dual-auth member authentication misidentified"
            assert native["local_login_enabled"] is True and native["created_at"],"Profile availability metadata missing"
            assert "password_hash" not in json.dumps(directory) and client_secret not in json.dumps(directory),"Member profiles exposed credentials"
            def start(browser=False):
                status,started,headers=request(f"/api/v1/auth/oidc/providers/{provider_id}/start",{},browser=browser)
                assert status==200,"OIDC start failed"
                params={k:v[0] for k,v in parse_qs(urlsplit(started["authorization_url"]).query).items()};assert params["scope"].split().count("openid")==1 and params["code_challenge_method"]=="S256","Missing OIDC PKCE"
                cookie="; ".join(v.split(";",1)[0] for v in headers.get_all("Set-Cookie"));assert "HttpOnly" in headers.get("Set-Cookie") and "SameSite=Lax" in headers.get("Set-Cookie"),"Correlation cookie policy failed"
                code=secrets.token_urlsafe(24);codes[code]=params
                return "/api/v1/auth/oidc/callback?"+urlencode({"code":code,"state":params["state"]}),cookie
            status,owner_key,_=request(f"/api/v1/actors/{owner_id}/api-keys",{"name":"Independent MCP credential","scope":{"capabilities":["projects:read"],"project_ids":[],"all_projects":True,"installation":False},"expires_in_seconds":3600},owner);assert status==201,"Owner key setup failed"
            callback,cookie=start();assert request(callback)[0]==401,"Missing browser proof accepted"
            assert request(callback,cookie="nexora_oidc_invalid=wrong")[0]==401,"Wrong browser proof accepted"
            assert request(callback,cookie=cookie.split("=",1)[0]+"=wrong-proof")[0]==401,"Wrong correlation hash accepted"
            parts=urlsplit(callback);wrong=parse_qs(parts.query);wrong["state"][0]+="x"
            assert request(parts.path+"?"+urlencode({key:values[0] for key,values in wrong.items()}),cookie=cookie)[0]==401,"Modified state accepted"
            server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,"Pending OIDC restart shutdown failed";server.stderr.close()
            server=subprocess.Popen(args+["serve","--bind",f"127.0.0.1:{api_port}","--identity-policy",str(policy_file)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
            assert server.stderr.readline().startswith("nexora-api listening"),"Pending OIDC restart failed"
            status,oauth_session,_=request(callback,cookie=cookie);assert status==200,"Valid OIDC login failed";assert oauth_session["actor_id"]==person["id"],"Wrong corporate actor"
            status,oauth_session,_=request("/api/v1/auth/gate",{"action":"skip"},oauth_session["access_token"]);assert status==200
            assert request(callback,cookie=cookie)[0]==401,"OIDC callback replay accepted"
            status,_,_=request("/api/v1/projects",{"name":"Role claim escalation"},oauth_session["access_token"]);assert status==403,"IdP roles became Nexora grants"
            status,contact,_=request("/api/v1/me/email",token=oauth_session["access_token"])
            assert status==200 and contact["email"]=="" and contact["suggested_email"]=="same-email@example.invalid","Unverified OIDC email became an account address"
            mode="email_verified";callback,cookie=start();status,verified_session,_=request(callback,cookie=cookie);assert status==200
            status,verified_session,_=request("/api/v1/auth/gate",{"action":"skip"},verified_session["access_token"]);assert status==200
            status,contact,_=request("/api/v1/me/email",token=verified_session["access_token"])
            assert status==200 and contact["email"]=="verified@example.invalid","Existing OIDC account email was not backfilled"
            mode="preferred_contact";callback,cookie=start();status,preferred_session,_=request(callback,cookie=cookie);assert status==200
            status,preferred_session,_=request("/api/v1/auth/gate",{"action":"skip"},preferred_session["access_token"]);assert status==200
            status,contact,_=request("/api/v1/me/email",token=preferred_session["access_token"])
            assert status==200 and contact["email"]=="verified@example.invalid" and contact["suggested_email"]=="preferred@example.invalid","Unverified preferred username overwrote a configured email"
            mode="valid"
            callback,cookie=start(browser=True)
            status,browser_session,browser_headers=request(callback,cookie=cookie)
            assert status==200 and browser_session["browser_session"] and "access_token" not in browser_session,"Browser OIDC leaked a bearer"
            session_cookie=next(v.split(";",1)[0] for v in browser_headers.get_all("Set-Cookie") if v.startswith("nexora_dev_session="))
            status,browser_ready,browser_headers=request("/api/v1/auth/gate",{"action":"skip"},cookie=session_cookie,browser=True)
            assert status==200 and browser_ready["stage"]=="ready" and "access_token" not in browser_ready,"Browser OIDC gate failed"
            session_cookie=next(v.split(";",1)[0] for v in browser_headers.get_all("Set-Cookie") if v.startswith("nexora_dev_session="))
            assert request("/api/v1/auth/session",cookie=session_cookie,browser=True)[0]==200,"OIDC cookie unavailable in another tab"
            assert request("/api/v1/auth/session",cookie=session_cookie,browser=True,method="DELETE")[0]==204,"OIDC cookie logout failed"
            callback,cookie=start()
            with ThreadPoolExecutor(max_workers=2) as workers:
                statuses=list(workers.map(lambda _:request(callback,cookie=cookie)[0],range(2)))
            assert sorted(statuses)==[200,401],"Concurrent callback created more than one session"
            callback,cookie=start()
            status,provider_meta,_=request("/api/v1/auth/oidc/providers",{"name":"Corporate directory updated","issuer":issuer,"client_id":"fixture-client","client_secret":client_secret,"redirect_uri":base+"/api/v1/auth/oidc/callback","enabled":True,"expected_revision":provider_meta["revision"]},owner);assert status==200,"OIDC configuration revision failed"
            assert request(callback,cookie=cookie)[0]==401,"Old provider revision remained authorized"
            for invalid in ["issuer","audience","nonce","expired","signature","access_hash","future_iat"]:
                mode=invalid;callback,cookie=start();assert request(callback,cookie=cookie)[0]==401,"Invalid signed identity accepted"
            mode="valid";subject="unregistered-same-email";callback,cookie=start();status,pending,_=request(callback,cookie=cookie);assert status==200 and pending["next_step"]=="pending_role" and pending["actor_id"]!=person["id"],"New subject was not isolated"
            assert request("/api/v1/projects",token=pending["access_token"])[0]==401,"Pending account accessed API"
            assert request("/api/v1/auth/gate",{"action":"skip"},pending["access_token"])[0]==403,"Pending account skipped authorization"
            subject="corporate-owner";callback,cookie=start();status,owner_oidc,_=request(callback,cookie=cookie);assert status==200,"Corporate owner login failed"
            status,owner_oidc,_=request("/api/v1/auth/gate",{"action":"skip"},owner_oidc["access_token"]);assert status==200
            status,_,_=request("/api/v1/auth/methods",{"expected_revision":1,"local_enabled":False},owner_oidc["access_token"],method="PUT");assert status==200,"Corporate-only policy failed"
            assert request("/api/v1/auth/login",{"username":"owner","password":password})[0]==401,"Local password bypassed policy"
            assert request("/api/v1/projects",token=owner)[0]==401,"Previous local session survived corporate-only policy"
            assert request("/api/v1/projects",token=owner_oidc["access_token"])[0]==200,"Corporate session did not survive policy"
            assert request("/api/v1/projects",token=owner_key["key"])[0]==200,"SSO-only policy revoked independent API key"
            status,_,_=request("/api/v1/auth/oidc/providers",{"name":"Corporate directory","issuer":issuer,"client_id":"fixture-client","client_secret":client_secret,"redirect_uri":base+"/api/v1/auth/oidc/callback","enabled":False,"expected_revision":provider_meta["revision"]},owner_oidc["access_token"]);assert status==409,"Last corporate administrator could be disabled"
            assert request(provider_path,{"enabled":False,"expected_revision":provider_meta["revision"]},owner_oidc["access_token"],method="PATCH")[0]==409,"PATCH disabled the last corporate administrator"
            status,_,_=request(f"/api/v1/actors/{person['id']}/suspend",{},owner_oidc["access_token"]);assert status==204,"Suspend failed"
            subject="corporate-person";callback,cookie=start();assert request(callback,cookie=cookie)[0]==401,"Suspended user logged in through OIDC"
            assert request("/api/v1/projects",token=oauth_session["access_token"])[0]==401,"Suspended corporate session remained usable"
            status,_,_=request("/api/v1/auth/methods",{"expected_revision":2,"local_enabled":True},owner_oidc["access_token"],method="PUT");assert status==200,"Restoring local login failed"
            status,native,_=request("/api/v1/auth/login",{"username":"owner","password":password});assert status==200,"Restored password login failed"
            status,native,_=request("/api/v1/auth/gate",{"action":"skip"},native["access_token"]);assert status==200
            assert request("/api/v1/projects",token=owner)[0]==401,"Restoring policy resurrected revoked session"
            status,_,_=request(provider_path,{"enabled":False,"expected_revision":provider_meta["revision"]},native["access_token"],method="PATCH");assert status==200,"Disabling provider with local fallback failed"
            assert request("/api/v1/projects",token=owner_oidc["access_token"])[0]==401,"Disabled provider session remained usable"
            assert request(f"/api/v1/auth/oidc/providers/{provider_id}/start",{})[0]==401,"Disabled IdP started authorization"
        finally:
            server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,"OIDC final shutdown failed";server.stderr.close()
    provider.shutdown();provider.server_close()
    print(json.dumps({"passed":True,"live_corporate_provider":False,"identity":"OIDC issuer/subject","git_authorization_unchanged":True}))
try:main()
except Exception as error:
    import traceback
    for frame in traceback.extract_tb(error.__traceback__):print(f"{Path(frame.filename).name}:{frame.lineno} {frame.name}",file=sys.stderr)
    print(type(error).__name__,file=sys.stderr)
    if isinstance(error,AssertionError):print(str(error),file=sys.stderr)
    print("OIDC fixture failed; credential/token content suppressed",file=sys.stderr);sys.exit(1)
