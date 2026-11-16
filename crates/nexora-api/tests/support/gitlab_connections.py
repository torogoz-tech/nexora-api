"""Read-only Git provider fixture. No live repositories or reusable credentials."""
import base64,hashlib,hmac,json,secrets,signal,socket,subprocess,sys,tempfile,threading,time
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import parse_qs,urlsplit,urlencode,unquote
import urllib.request,urllib.error
b64=lambda data:base64.urlsafe_b64encode(data).rstrip(b"=").decode()
client_secret=secrets.token_urlsafe(32);codes={};tokens={};refreshes={};revoked=False;refresh_count=0;branches_reject=False;token_scope="read_api read_repository"
projects={"team/backend":101,"team/frontend":102,"renamed/backend":101,"team/metadata-only":103}
canonical_backend='team/backend'
provider_control={'available':True}
profile_control={'pr_status':200,'pause':False,'entered':threading.Event(),'release':threading.Event()}
discovery_control={'pause':False,'status':200,'entered':threading.Event(),'release':threading.Event()}
pause_proof=False;proof_entered=threading.Event();proof_release=threading.Event()
class GitLab(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def reply(self,value,status=200):
  data=json.dumps(value).encode();self.send_response(status);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
 def do_POST(self):
  global refresh_count
  if not provider_control['available']:self.reply({},503);return
  if self.path!="/oauth/token":self.reply({},404);return
  form={key:values[0] for key,values in parse_qs(self.rfile.read(int(self.headers["Content-Length"])).decode()).items()}
  if form.get("client_secret")!=client_secret:self.reply({"error":"invalid_client"},401);return
  if form.get("grant_type")=="refresh_token":
   if revoked or not refreshes.pop(form.get("refresh_token"),None):self.reply({"error":"invalid_grant"},400);return
   refresh_count+=1
  else:
   original=codes.pop(form.get("code"),None)
   if not original or b64(hashlib.sha256(form.get("code_verifier","").encode()).digest())!=original["code_challenge"]:self.reply({"error":"invalid_grant"},400);return
  access=secrets.token_urlsafe(32);refresh=secrets.token_urlsafe(32);tokens[access]=True;refreshes[refresh]=True
  self.reply({"access_token":access,"refresh_token":refresh,"token_type":"Bearer","scope":token_scope,"expires_in":7200 if form.get("grant_type")=="refresh_token" else 10})
 def do_GET(self):
  if not provider_control['available']:self.reply({},503);return
  if revoked or not tokens.get(self.headers.get("Authorization","").removeprefix("Bearer ")):self.reply({},401);return
  path=unquote(urlsplit(self.path).path)
  if path=="/api/v4/user":self.reply({"id":42,"username":"source-account","state":"active"});return
  if path=="/api/v4/projects":
   if discovery_control['pause']:discovery_control['entered'].set();assert discovery_control['release'].wait(timeout=20),'Discovery gate not released'
   if discovery_control['status']!=200:self.reply({},discovery_control['status']);return
   query=parse_qs(urlsplit(self.path).query);assert query.get('membership')==['true'] and query.get('simple')==['true'],'Discovery did not restrict user project membership'
   rows=[{'id':remote,'path_with_namespace':name,'http_url_to_repo':'https://forbidden.example.invalid/secret'} for name,remote in [(canonical_backend,101),('team/frontend',102),('team/metadata-only',103)]]
   page=int(query.get('page',['1'])[0]);limit=int(query.get('per_page',['100'])[0]);self.reply(rows[(page-1)*limit:page*limit]);return
  if path.endswith("/repository/branches"):
   if pause_proof:proof_entered.set();assert proof_release.wait(timeout=20),"Delegated source proof was not released"
   if branches_reject:self.reply({},401);return
   if path=="/api/v4/projects/103/repository/branches":self.reply({},403);return
   self.reply([]);return
  if path=="/api/v4/projects/101/merge_requests":
   if profile_control['pause']:profile_control['entered'].set();assert profile_control['release'].wait(timeout=20),'Profile provider gate not released'
   if profile_control['pr_status']!=200:self.reply({},profile_control['pr_status']);return
   self.reply([{'iid':7,'title':'Review backend change','state':'opened','web_url':'https://gitlab.com/team/backend/-/merge_requests/7'}]);return
  full_name=path.removeprefix("/api/v4/projects/")
  if full_name.isdigit():
   names=[name for name,remote in projects.items() if str(remote)==full_name]
   if names:full_name=canonical_backend if full_name=='101' and projects.get(canonical_backend)==101 else names[0]
  if full_name in projects:self.reply({"id":projects[full_name],"path_with_namespace":full_name,"default_branch":"main","repository_access_level":"enabled","description":"First line\nSecond line","web_url":"https://gitlab.com/"+full_name});return
  self.reply({},404)
def main():
 global revoked,branches_reject,token_scope,pause_proof,canonical_backend
 mode=sys.argv[2] if len(sys.argv)>2 else "core"
 binary=str(Path(sys.argv[1]).resolve());workspace=Path(__file__).resolve().parents[4]
 provider=ThreadingHTTPServer(("127.0.0.1",0),GitLab);threading.Thread(target=provider.serve_forever,daemon=True).start();origin=f"http://127.0.0.1:{provider.server_port}"
 with socket.socket() as probe:probe.bind(("127.0.0.1",0));port=probe.getsockname()[1]
 base=f"http://127.0.0.1:{port}";namespace="nexora_platform_gitlab_"+str(time.time_ns());args=[binary,"--connections",str(workspace/".nexora/local/connections.json"),"--schema",namespace]
 http=urllib.request.build_opener(urllib.request.ProxyHandler({}))
 def request(path,body=None,token=None,cookie=None,method=None,idempotency=None,extra_headers=None,raw=None):
  headers={"Content-Type":"application/json"}
  if idempotency:headers["Idempotency-Key"]=idempotency
  if token:headers["Authorization"]="Bearer "+token
  if cookie:headers["Cookie"]=cookie
  headers.update(extra_headers or {})
  req=urllib.request.Request(base+path,data=raw if raw is not None else (None if body is None else json.dumps(body).encode()),headers=headers,method=method or ("POST" if body is not None else "GET"))
  try:reply=http.open(req,timeout=30)
  except urllib.error.HTTPError as error:reply=error
  raw=reply.read();return reply.status,json.loads(raw) if raw else None,reply.headers
 with tempfile.TemporaryDirectory(prefix="nexora-gitlab-") as folder:
  password=secrets.token_urlsafe(32);pw=Path(folder)/"password";pw.write_text(password);pw.chmod(0o600)
  for command in [["migrate"],["bootstrap","--organization","Git fixture","--username","owner","--name","Owner","--password-file",str(pw)]]:
   try:result=subprocess.run(args+command,capture_output=True,text=True,timeout=60)
   except subprocess.TimeoutExpired:raise AssertionError('Git fixture '+command[0]+' exceeded 60 seconds') from None
   safe_reason='STORAGE_UNAVAILABLE' if 'platform storage unavailable' in result.stderr else ('CAPACITY_UNAVAILABLE' if 'platform capacity unavailable' in result.stderr else 'OTHER_FAILURE')
   assert result.returncode==0,'Git fixture '+command[0]+' initialization failed: '+safe_reason
  server=subprocess.Popen(args+["serve","--bind",f"127.0.0.1:{port}"],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
  try:
   assert server.stderr.readline().startswith("nexora-api listening"),"Git fixture start failed"
   status,session,_=request("/api/v1/auth/login",{"username":"owner","password":password});assert status==200,"Git fixture login failed";owner=session["access_token"]
   assert request("/api/v1/git/connections",token=owner)[0]==200,"Git connection API missing"
   if mode=="webhook":assert request("/api/v1/git/gitlab/webhooks/probe",{},method="POST")[0]!=404,"GitLab webhook API missing"
  finally:server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,"Git initial stop failed";server.stderr.close()
  policy=Path(folder)/"git-policy.json";policy.write_text(json.dumps({"gitlab_origin":origin,"callback_origin":base}))
  server=subprocess.Popen(args+["serve","--bind",f"127.0.0.1:{port}","--git-policy",str(policy)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
  try:
   assert server.stderr.readline().startswith("nexora-api listening"),"Git policy startup failed"
   if mode=="catalog":assert request('/api/v1/git/gitlab/configuration',token=owner)[0]==404,"Missing GitLab configuration cannot be read as metadata"
   configuration_input={"client_id":"fixture-app","client_secret":client_secret,"redirect_uri":base+"/api/v1/git/gitlab/callback","enabled":True,"expected_revision":0}
   status,configuration,_=request("/api/v1/git/gitlab/configuration",configuration_input,owner);assert status==201,"GitLab application configuration failed"
   if mode=="catalog":
    status,saved,_=request('/api/v1/git/gitlab/configuration',token=owner)
    assert status==200 and saved==configuration and 'client_secret' not in saved,"GitLab metadata read changed or leaked configuration"
    assert request('/api/v1/git/gitlab/configuration')[0]==401,"Unauthenticated actor read provider configuration"
   status,options,_=request("/api/v1/auth/methods");assert status==200 and options["oidc"]==[],"Git consent became corporate login"
   def start(token=owner):
    status,flow,headers=request("/api/v1/git/gitlab/start",{},token);assert status==200,"Git consent start failed"
    params={key:value[0] for key,value in parse_qs(urlsplit(flow["authorization_url"]).query).items()};assert set(params["scope"].split())=={"api"} and params["code_challenge_method"]=="S256","Wrong Git OAuth scopes"
    code=secrets.token_urlsafe(24);codes[code]=params;cookie=headers["Set-Cookie"].split(";",1)[0]
    return "/api/v1/git/gitlab/callback?"+urlencode({"code":code,"state":params["state"]}),cookie
   callback,cookie=start();assert request(callback)[0]==401,"Git callback accepted missing browser proof"
   status,connection,_=request(callback,cookie=cookie);assert status==200,"Git callback verification failed";connection_id=connection["id"];assert connection["remote_account_id"]=="42" and connection["status"]=="active","Unverified remote connection"
   assert request(callback,cookie=cookie)[0]==401,"Git callback replay accepted"
   status,listing,_=request("/api/v1/git/connections",token=owner);assert status==200 and client_secret not in json.dumps(listing) and all(secret not in json.dumps(listing) for secret in tokens),"Provider tokens leaked"
   if mode=='discovery':
    from verify_remote_repository_pages import verify_remote_repository_pages
    verify_remote_repository_pages(request,f'/api/v1/git/connections/{connection_id}/repositories',owner,['101','102','103'])
   if mode=='profile':
    from verify_repository_profiles import verify_unlinked_profile
    profile_route=verify_unlinked_profile(request,connection_id,owner,'gitlab')
    profile_control['pr_status']=403
    unavailable=request(profile_route,token=owner)
    assert unavailable[0]==200 and unavailable[1]['pull_requests_available'] is False and unavailable[1]['pull_requests']==[],'PR permission denial hid repository metadata'
    profile_control['pr_status']=200
    from verify_repository_profiles import verify_profile_live_authority
    verify_profile_live_authority(request,profile_route,owner,session['actor_id'],profile_control)
   project_ids=[];repository_ids=[]
   for name,full_name in [("Backend project","team/backend"),("Frontend project","team/frontend")]:
    status,project,_=request("/api/v1/projects",{"name":name},owner);assert status==201,"Project setup failed";project_ids.append(project["id"])
    status,repo,_=request(f"/api/v1/projects/{project['id']}/repositories",{"provider":"gitlab","full_name":full_name},owner);assert status==201,"Repository draft failed";repository_ids.append(repo["id"])
    binding={"connection_id":connection_id,"expected_revision":1}
    if mode=='discovery':
     if len(project_ids)==1:
      from verify_remote_repository_pages import verify_remote_repository_pages
      verify_remote_repository_pages(request,f"/api/v1/projects/{project['id']}/git/connections/{connection_id}/repositories",owner,['101','102','103'])
      assert request(f"/api/v1/projects/{project['id']}/repositories/{repo['id']}",token=owner)[1]['authorization_status']=='awaiting_authorization','Discovery granted source authority'
      canonical_backend='moved/renamed';projects[canonical_backend]=101;projects['team/backend']=999
     binding['expected_remote_id']='101' if len(project_ids)==1 else '102'
    status,bound,_=request(f"/api/v1/projects/{project['id']}/repositories/{repo['id']}/authorize",binding,owner,idempotency="bind-"+repo["id"]);assert status==200 and bound["authorization_status"]=="authorized" and bound["connection_id"]==connection_id,"Remote repository was not verified"
    if mode=='discovery':assert bound['remote_id']==binding['expected_remote_id'],'Selection followed reused repository name'
   if mode=='profile':
    from verify_repository_profiles import verify_linked_profile
    registered_profile=verify_linked_profile(request,connection_id,project_ids[0],repository_ids[0],owner)
   assert refresh_count==1,"Expiring credentials were not renewed exactly once"
   status,metadata,_=request(f"/api/v1/projects/{project_ids[0]}/repositories",{"provider":"gitlab","full_name":"team/metadata-only"},owner);assert status==201,"Metadata draft failed"
   assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{metadata['id']}/authorize",{"connection_id":connection_id,"expected_revision":1},owner)[0]==403,"Public metadata was mistaken for repository read access"
   assert len(project_ids)==2 and project_ids[0]!=project_ids[1],"Connection was not shared across project bindings"
   if mode=="webhook":
    from concurrent.futures import ThreadPoolExecutor
    rawkey=secrets.token_bytes(32);key='whsec_'+base64.b64encode(rawkey).decode();prefix=f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}/webhooks/gitlab"
    status,subscription,_=request(prefix,{"signing_token":key,"expected_revision":2},owner);assert status==201 and key not in json.dumps(subscription),"GitLab webhook registration failed";hook=subscription['id']
    status,listing,_=request(prefix,token=owner);assert status==200 and key not in json.dumps(listing),"Webhook metadata leaked signing key"
    endpoint='/api/v1/git/gitlab/webhooks/'+hook
    def deliver(payload,delivery='gitlab-push',timestamp=None,tamper=False,signed=True,multi=False):
     timestamp=str(int(time.time()) if timestamp is None else timestamp);raw=json.dumps(payload,separators=(',',':')).encode();sig='v1,'+base64.b64encode(hmac.new(rawkey,delivery.encode()+b'.'+timestamp.encode()+b'.'+raw,hashlib.sha256).digest()).decode()
     headers={'webhook-id':delivery,'webhook-timestamp':timestamp,'X-Gitlab-Event':'Push Hook'}
     if signed:headers['webhook-signature']=('v1,AAAA ' if multi else '')+sig
     return request(endpoint,method='POST',raw=raw+(b' ' if tamper else b''),extra_headers=headers)
    push={'object_kind':'push','project_id':101,'project':{'id':101},'ref':'refs/heads/main','after':'d'*40}
    assert deliver(push,signed=False)[0]==401,"Unsigned GitLab event accepted"
    assert deliver(push,tamper=True)[0]==401,"GitLab signature accepted altered body"
    assert deliver(push,timestamp=int(time.time())-601)[0]==401,"GitLab accepted stale signed timestamp"
    assert deliver(push,timestamp=int(time.time())+601)[0]==401,"GitLab accepted future signed timestamp"
    assert deliver({**push,'project_id':102,'project':{'id':102}},delivery='other-project')[0]==403,"Webhook secret authorized another source"
    status,result,_=deliver(push);assert status==202 and result['duplicate']==False,"Signed GitLab push rejected"
    assert deliver(push)[1]['duplicate']==True,"GitLab retry was not deduplicated"
    assert deliver(push,multi=True)[1]['duplicate']==True,"GitLab signature list did not accept valid candidate"
    assert deliver({**push,'after':'e'*40})[0]==409,"GitLab delivery changed content"
    with ThreadPoolExecutor(max_workers=2) as pool:results=list(pool.map(lambda _:deliver(push,delivery='gitlab-race'),range(2)))
    assert all(v[0]==202 for v in results) and sorted(v[1]['duplicate'] for v in results)==[False,True],"GitLab concurrent delivery duplicated effect"
    current=request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)[1];assert current['source_generation']==2 and current['latest_source_commit']=='d'*40,"GitLab source generation incorrect"
    assert request(f"/api/v1/projects/{project_ids[1]}/repositories/{repository_ids[1]}",token=owner)[1]['source_generation']==0,"GitLab hook changed another repository"
    assert request(prefix+'/'+hook,token=owner,method='DELETE')[0]==204,"GitLab hook revocation failed"
    assert deliver(push,delivery='after-revoke')[0]==401,"Revoked GitLab hook accepted delivery"
   status,hidden,_=request(f"/api/v1/projects/{project_ids[0]}/repositories",{"provider":"gitlab","full_name":"private/inaccessible"},owner);assert status==201,"Hidden draft failed"
   assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{hidden['id']}/authorize",{"connection_id":connection_id,"expected_revision":1},owner)[0]==404,"Unreachable provider repository accepted"
   status,actor,_=request("/api/v1/actors",{"kind":"agent","name":"Project developer","purpose":"Scoped connection use"},owner);assert status==201,"Developer setup failed"
   for capability in ["repositories:read","repositories:write"]:assert request(f"/api/v1/actors/{actor['id']}/grants",{"capability":capability,"project_id":project_ids[0]},owner)[0]==204,"Developer grants failed"
   status,issued,_=request(f"/api/v1/actors/{actor['id']}/api-keys",{"name":"Scoped developer","scope":{"capabilities":["repositories:read","repositories:write"],"project_ids":[project_ids[0]],"all_projects":False,"installation":False},"expires_in_seconds":3600},owner);assert status==201,"Developer key failed";developer=issued["key"]
   if mode=='profile':
    assert request(profile_route,token=developer)[0]==403,'Project-scoped key discovered global private profile'
    assert request(registered_profile,token=developer)[0]==404,'Missing knowledge capability read registered profile'
    assert request(f'/api/v1/projects/{project_ids[1]}/repositories/{repository_ids[1]}/profile',token=developer)[0]==404,'Repository profile bypassed project scope'
   assert request(f"/api/v1/projects/{project_ids[1]}/repositories/{repository_ids[1]}",token=developer)[0]==404,"Connection bypassed project scope"
   status,extra,_=request(f"/api/v1/projects/{project_ids[0]}/repositories",{"provider":"gitlab","full_name":"renamed/backend"},owner);assert status==201,"Extra draft failed"
   if mode=="catalog":
    assert request('/api/v1/git/gitlab/configuration',token=developer)[0]==403,"Project writer read installation configuration"
    assert request('/api/v1/git/connections',token=developer)[0]==403,"Project writer read global connection catalog"
    assert request(f"/api/v1/projects/{project_ids[0]}/git/connections?limit=1",token=developer)[1]['items']==[],"Project writer discovered an undelegated private connection"
    assert request(f"/api/v1/projects/{project_ids[1]}/git/connections",token=developer)[0] in [403,404],"Project connection catalog bypassed actor scope"
   assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{extra['id']}/authorize",{"connection_id":connection_id,"expected_revision":1},developer)[0]==403,"Project writer implicitly borrowed another actor connection"
   assert request(f"/api/v1/git/connections/{connection_id}/projects/{project_ids[0]}",{},owner)[0]==204,"Explicit project delegation failed"
   if mode=="catalog":
    status,available,_=request(f"/api/v1/projects/{project_ids[0]}/git/connections?limit=1",token=developer)
    assert status==200 and [c['id'] for c in available['items']]==[connection_id] and available['next_cursor'] is None,"Delegated project connection was not discoverable"
   # Same remote ID already belongs to an inscription: delegation must not duplicate it.
   assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{extra['id']}/authorize",{"connection_id":connection_id,"expected_revision":1},developer)[0]==409,"Remote identity duplicate was not detected"
   if mode=='discovery':
    from verify_remote_discovery_revocation import verify_remote_discovery_revocation
    verify_remote_discovery_revocation(request,f'/api/v1/projects/{project_ids[0]}/git/connections/{connection_id}/repositories',owner,developer,f'/api/v1/git/connections/{connection_id}/projects/{project_ids[0]}',connection_id,discovery_control)
    assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)[1]['authorization_status']=='revoked','Rejected discovery credential retained bound source'
   if mode=="rename":
    del projects['team/backend'];del projects['renamed/backend'];projects['moved/renamed']=101;projects['team/backend']=999
    route=f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}"
    status,renamed,_=request(route+'/authorize',{"connection_id":connection_id,"expected_revision":2},owner)
    assert status==200,"Renamed GitLab repository was not resolved by stable remote identity"
    assert renamed['id']==repository_ids[0] and renamed['remote_id']=='101' and renamed['remote_full_name']=='moved/renamed' and renamed['requested_full_name']=='team/backend' and renamed['revision']==3,"Rename replaced registered identity or followed reused old name"
   if mode=="delegation":
    delegation=f"/api/v1/git/connections/{connection_id}/projects/{project_ids[0]}"
    assert request(delegation,token=developer,method="DELETE")[0]==403,"Delegated writer withdrew source owner's grant"
    assert request(delegation,token=owner,method="DELETE",idempotency="withdraw-project")[0]==204,"Project delegation withdrawal failed"
    assert request(f"/api/v1/projects/{project_ids[0]}/git/connections",token=developer)[1]['items']==[],"Connection catalog cached a withdrawn project delegation"
    first=request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)[1]
    other=request(f"/api/v1/projects/{project_ids[1]}/repositories/{repository_ids[1]}",token=owner)[1]
    assert first['authorization_status']=='revoked' and first['revision']==3,"Withdrawal retained project source authority"
    assert other['authorization_status']=='authorized' and other['revision']==2,"Withdrawal affected a different project"
    assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{extra['id']}/authorize",{"connection_id":connection_id,"expected_revision":1},developer)[0]==403,"Withdrawn delegate still borrowed source credentials"
    assert request(delegation,{},owner)[0]==204,"Delegation could not be granted again"
    assert [c['id'] for c in request(f"/api/v1/projects/{project_ids[0]}/git/connections",token=developer)[1]['items']]==[connection_id],"Connection catalog did not observe a new project delegation"
    assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}/authorize",{"connection_id":connection_id,"expected_revision":3},developer)[0]==200,"Fresh delegation proof could not restore source"
    assert request(delegation,token=owner,method="DELETE",idempotency="withdraw-project")[0]==204,"Withdrawal replay failed"
    assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)[1]['authorization_status']=='authorized',"Historical withdrawal receipt removed newly granted source authority"
    from concurrent.futures import ThreadPoolExecutor
    pause_proof=True
    with ThreadPoolExecutor(max_workers=1) as pool:
     pending=pool.submit(request,f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}/authorize",{"connection_id":connection_id,"expected_revision":4},developer)
     try:
      assert proof_entered.wait(timeout=20),"Delegated proof did not reach controlled gate"
      assert request(delegation,token=owner,method="DELETE")[0]==204,"Concurrent delegation withdrawal failed"
     finally:pause_proof=False;proof_release.set()
     assert pending.result(timeout=30)[0]==403,"Proof retained a withdrawn delegation"
    assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)[1]['authorization_status']=='revoked',"In-flight proof restored revoked project authority"
    assert request(f"/api/v1/projects/{project_ids[1]}/repositories/{repository_ids[1]}",token=owner)[1]['authorization_status']=='authorized',"Concurrent withdrawal affected another project"
   if mode=='state':
    from verify_provider_configuration_state import verify_provider_configuration_state
    stale_callback,stale_cookie=start()
    provider_control['available']=False
    try:verify_provider_configuration_state(request,'/api/v1/git/gitlab/configuration',owner,developer,connection_id,list(zip(project_ids,repository_ids)))
    finally:provider_control['available']=True
    assert request(stale_callback,cookie=stale_cookie)[0]==401,'State change retained stale GitLab consent'
    fresh_callback,fresh_cookie=start();status,fresh_connection,_=request(fresh_callback,cookie=fresh_cookie);assert status==200,'Re-enabled GitLab credentials could not create new connection'
    route=f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}/authorize"
    assert request(route,{'connection_id':fresh_connection['id'],'expected_revision':3},owner)[0]==200,'Fresh source proof did not restore GitLab binding'
    assert request('/api/v1/git/gitlab/configuration',{**configuration_input,'expected_revision':3},owner)[0]==200,'Credential replacement after state change failed'
    assert request(route,{'connection_id':fresh_connection['id'],'expected_revision':4},owner)[0]==200,'Credential context did not survive different administrative revision'
    status,replayed,_=request('/api/v1/git/gitlab/configuration',{'enabled':False,'expected_revision':1},owner,method='PATCH',extra_headers={'Idempotency-Key':'state-disable-once'});assert status==200 and replayed['revision']==2,'State receipt did not replay'
    assert request('/api/v1/git/gitlab/configuration',token=owner)[1]['enabled']==True and request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)[1]['authorization_status']=='authorized','Historical state receipt revoked newly granted authority'
    assert request(f"/api/v1/git/connections/{fresh_connection['id']}",token=owner,method='DELETE')[0]==204,'Fresh GitLab state fixture cleanup failed'
   callback2,cookie2=start();status,second,_=request(callback2,cookie=cookie2);assert status==200,"Second connection failed"
   # A provider can reject the credential on the code endpoint after user identity succeeds.
   branches_reject=True
   status,_,_=request(f"/api/v1/projects/{project_ids[0]}/repositories/{metadata['id']}/authorize",{"connection_id":second['id'],"expected_revision":1},owner)
   branches_reject=False
   assert status==401,"Code endpoint did not reject invalid credential"
   failures=[]
   _,listing,_=request("/api/v1/git/connections",token=owner)
   if next(c for c in listing["items"] if c["id"]==second["id"])["status"]!="revoked":failures.append("Code endpoint credential rejection did not revoke connection")
   for scope in ["read_api", "read_api read_repository sudo"]:
    token_scope=scope;bad_callback,bad_cookie=start()
    assert request(bad_callback,cookie=bad_cookie)[0]==403,"Invalid provider scopes accepted"
    assert request(bad_callback,cookie=bad_cookie)[0]==401,"Rejected scope callback remained reusable"
   token_scope="read_api read_repository"
   if mode=="catalog":
    from verify_git_connection_pages import verify_git_connection_pages
    verify_git_connection_pages(request,owner,[connection_id,second['id']])
    assert request('/api/v1/git/gitlab/configuration',{**configuration_input,'enabled':False,'expected_revision':1},owner)[0]==200,"GitLab configuration disable failed"
    status,disabled,_=request('/api/v1/git/gitlab/configuration',token=owner)
    assert status==200 and disabled['enabled']==False and disabled['revision']==2,"Disabled GitLab configuration metadata unavailable"
    assert request(f"/api/v1/projects/{project_ids[0]}/git/connections",token=developer)[1]['items']==[],"Project catalog retained revoked source authority"
   status,_,_=request(f"/api/v1/git/connections/{connection_id}",token=owner,method="DELETE");assert status==204,"Connection revocation failed"
   status,repo,_=request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=developer);assert status==200 and repo["authorization_status"]=="revoked","Connection revocation did not invalidate repository authority"
   assert request(f"/api/v1/projects/{project_ids[0]}/repositories/{hidden['id']}/authorize",{"connection_id":connection_id,"expected_revision":1},owner)[0] in [401,409],"Revoked connection remained usable"
   if mode=='profile':
    assert request(profile_route,token=owner)[0] in [401,409],'Revoked connection retained global profile'
    assert request(registered_profile,token=owner)[0]==404,'Revoked source retained registered profile'
   revoked=True
   replay_input={"connection_id":connection_id,"expected_revision":1}
   if mode=='discovery':replay_input['expected_remote_id']='101'
   status,replayed,_=request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}/authorize",replay_input,owner,idempotency="bind-"+repository_ids[0])
   if status!=200 or replayed.get("revision")!=2:failures.append("Confirmed authorization receipt could not replay with provider unavailable and connection revoked")
   status,current,_=request(f"/api/v1/projects/{project_ids[0]}/repositories/{repository_ids[0]}",token=owner)
   assert status==200 and current["authorization_status"]=="revoked","Historical replay revived current authority"
   assert not failures,"; ".join(failures)
  finally:server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,"Git final stop failed";server.stderr.close()
 provider.shutdown();provider.server_close();print(json.dumps({"passed":True,"provider":"gitlab-fixture","multiple_projects":True,"live_provider":False}))
try:main()
except Exception as error:
 import traceback
 for frame in traceback.extract_tb(error.__traceback__):print(f"{Path(frame.filename).name}:{frame.lineno} {frame.name}",file=sys.stderr)
 print(type(error).__name__,file=sys.stderr)
 if isinstance(error,AssertionError):print(str(error),file=sys.stderr)
 print("Git fixture failed; credential content suppressed",file=sys.stderr);sys.exit(1)
