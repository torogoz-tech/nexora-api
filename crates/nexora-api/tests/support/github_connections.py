# /// script
# requires-python = ">=3.11"
# dependencies = ["cryptography==46.0.3"]
# ///
"""Signed local GitHub App fixture; no live accounts, code execution or reusable secrets."""
import base64,hashlib,hmac,json,secrets,signal,socket,subprocess,sys,tempfile,threading,time
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import parse_qs,urlsplit,urlencode,unquote
import urllib.request,urllib.error
from cryptography.hazmat.primitives.asymmetric import rsa,padding
from cryptography.hazmat.primitives import serialization,hashes
b64=lambda data:base64.urlsafe_b64encode(data).rstrip(b"=").decode()
key=rsa.generate_private_key(public_exponent=65537,key_size=2048)
pem=key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.NoEncryption()).decode()
secret=secrets.token_urlsafe(32);codes={};users={};refreshes={};installation_tokens={};manifest_codes=set();webhook_secret=secrets.token_urlsafe(32);suspended=False;user_revoked=False;content_forbidden=False;refresh_count=0;pause_proof=False;proof_entered=threading.Event();proof_release=threading.Event()
repository_name='team/backend';old_name_reused=False
discovery_mode=False
provider_control={'available':True}
profile_control={'pr_status':200,'pause':False,'entered':threading.Event(),'release':threading.Event()}
discovery_control={'pause':False,'status':200,'entered':threading.Event(),'release':threading.Event()}
class GitHub(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def reply(self,data,status=200):
  raw=json.dumps(data).encode();self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def token(self):return self.headers.get('Authorization','').removeprefix('Bearer ')
 def app(self):
  try:
   a,b,c=self.token().split('.');header=json.loads(base64.urlsafe_b64decode(a+'==='));claims=json.loads(base64.urlsafe_b64decode(b+'==='))
   assert header['alg']=='RS256' and claims['iss']=='fixture-client' and time.time()-100<claims['iat']<=time.time() and time.time()<claims['exp']<=time.time()+600
   key.public_key().verify(base64.urlsafe_b64decode(c+'==='),(a+'.'+b).encode(),padding.PKCS1v15(),hashes.SHA256());return True
  except Exception:return False
 def user(self):return not user_revoked and self.token() in users
 def installation(self,n):return {'id':n,'app_id':88 if n!=9 else 99,'account':{'id':700+n,'login':'source-org'},'suspended_at':'now' if suspended else None,'permissions':{'contents':'write','metadata':'read','pull_requests':'write','actions':'read','checks':'read','deployments':'read','administration':'read'},'repository_selection':'selected'}
 def do_POST(self):
  global refresh_count
  if not provider_control['available']:self.reply({},503);return
  raw=self.rfile.read(int(self.headers.get('Content-Length','0')))
  if self.path.startswith('/app-manifests/') and self.path.endswith('/conversions'):
   code=self.path.split('/')[2]
   if code not in manifest_codes:self.reply({},404);return
   manifest_codes.remove(code)
   self.reply({'id':88,'slug':'nexora-fixture','client_id':'fixture-client','client_secret':secret,'pem':pem,'webhook_secret':webhook_secret},201);return
  if self.path=='/login/oauth/access_token':
   form={k:v[0] for k,v in parse_qs(raw.decode()).items()}
   if form.get('client_secret')!=secret:self.reply({},401);return
   if form.get('grant_type')=='refresh_token':
    if user_revoked or not refreshes.pop(form.get('refresh_token'),None):self.reply({},401);return
    refresh_count+=1
   else:
    old=codes.pop(form.get('code'),None)
    if not old or b64(hashlib.sha256(form.get('code_verifier','').encode()).digest())!=old['code_challenge']:self.reply({},401);return
   token=secrets.token_urlsafe(32);refresh=secrets.token_urlsafe(32);users[token]=True;refreshes[refresh]=True
   self.reply({'access_token':token,'refresh_token':refresh,'token_type':'bearer','scope':'','expires_in':28800,'refresh_token_expires_in':15897600});return
  if self.path=='/app/installations/7/access_tokens' and self.app() and not suspended:
   body=json.loads(raw);assert body['permissions']=={'contents':'read','metadata':'read'}, 'Installation token requested excessive permissions'
   if body['repository_ids']!=[101]:self.reply({},403);return
   token=secrets.token_urlsafe(32);installation_tokens[token]=True
   self.reply({'token':token,'expires_at':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime(time.time()+3600)),'permissions':body['permissions'],'repositories':[{'id':101,'full_name':repository_name}]},201);return
  self.reply({},404)
 def do_GET(self):
  if not provider_control['available']:self.reply({},503);return
  path=unquote(urlsplit(self.path).path)
  if path=='/app' and self.app():self.reply({'id':88,'slug':'nexora-fixture','permissions':{'metadata':'read','contents':'read','pull_requests':'read'}});return
  if path.startswith('/app/installations/') and self.app():
   try:n=int(path.rsplit('/',1)[1])
   except ValueError:self.reply({},404);return
   if n in [7,8,9]:self.reply(self.installation(n));return
  if path=='/user' and self.user():self.reply({'id':42,'login':'source-user'});return
  if path=='/user/installations' and self.user():self.reply({'total_count':2,'installations':[self.installation(7),self.installation(9)]});return
  if path=='/user/installations/7/repositories' and self.user() and not suspended:
   rows=[{'id':101,'full_name':repository_name}]
   if discovery_mode:rows.append({'id':103,'full_name':'team/metadata-only','clone_url':'https://forbidden.example.invalid/secret'})
   query=parse_qs(urlsplit(self.path).query);page=int(query.get('page',['1'])[0]);limit=int(query.get('per_page',['100'])[0])
   if 'page' in query:
    if discovery_control['pause']:discovery_control['entered'].set();assert discovery_control['release'].wait(timeout=20),'Discovery gate not released'
    if discovery_control['status']!=200:self.reply({},discovery_control['status']);return
   self.reply({'total_count':len(rows),'repositories':rows[(page-1)*limit:page*limit]});return
  if path==f'/repos/{repository_name}/pulls' and self.user():
   if profile_control['pause']:profile_control['entered'].set();assert profile_control['release'].wait(timeout=20),'Profile provider gate not released'
   if profile_control['pr_status']!=200:self.reply({},profile_control['pr_status']);return
   self.reply([{'number':7,'title':'Review backend change','state':'open','html_url':'https://github.com/team/backend/pull/7'}]);return
  if path=='/repos/team/backend' and old_name_reused and self.user():self.reply({'id':999,'full_name':'team/backend','disabled':False});return
  if path==f'/repos/{repository_name}' and (self.user() or self.token() in installation_tokens):self.reply({'id':101,'full_name':repository_name,'disabled':False,'description':'First line\nSecond line','default_branch':'main','html_url':'https://github.com/'+repository_name});return
  if path in [f'/repos/{repository_name}/branches',*(['/repos/team/backend/branches'] if old_name_reused else [])] and (self.user() or self.token() in installation_tokens):
   if pause_proof and self.token() in installation_tokens:proof_entered.set();assert proof_release.wait(timeout=20),'Scoped proof was not released'
   if content_forbidden:self.reply({},403);return
   self.reply([{'name':'main','commit':{'sha':'a'*40}}]);return
  self.reply({},401 if user_revoked else 404)
def main():
 global suspended,user_revoked,content_forbidden,pause_proof,repository_name,old_name_reused,discovery_mode
 mode=sys.argv[2] if len(sys.argv)>2 else 'manual'
 discovery_mode=mode=='discovery'
 binary=str(Path(sys.argv[1]).resolve());workspace=Path(__file__).resolve().parents[4]
 provider=ThreadingHTTPServer(('127.0.0.1',0),GitHub);threading.Thread(target=provider.serve_forever,daemon=True).start();origin=f'http://127.0.0.1:{provider.server_port}'
 with socket.socket() as p:p.bind(('127.0.0.1',0));port=p.getsockname()[1]
 base=f'http://127.0.0.1:{port}';ns='nexora_platform_github_'+str(time.time_ns());args=[binary,'--connections',str(workspace/'.nexora/local/connections.json'),'--schema',ns];http=urllib.request.build_opener(urllib.request.ProxyHandler({}))
 def request(path,body=None,token=None,cookie=None,method=None,idem=None,extra_headers=None,raw=None):
  headers={'Content-Type':'application/json'}
  if token:headers['Authorization']='Bearer '+token
  if cookie:headers['Cookie']=cookie
  if idem:headers['Idempotency-Key']=idem
  headers.update(extra_headers or {})
  req=urllib.request.Request(base+path,data=raw if raw is not None else (None if body is None else json.dumps(body).encode()),headers=headers,method=method or ('POST' if body is not None else 'GET'))
  try:response=http.open(req,timeout=30)
  except urllib.error.HTTPError as e:response=e
  raw=response.read();return response.status,json.loads(raw) if raw else None,response.headers
 with tempfile.TemporaryDirectory(prefix='nexora-github-') as folder:
  password=secrets.token_urlsafe(32);pw=Path(folder)/'password';pw.write_text(password);pw.chmod(0o600)
  for command in [['migrate'],['bootstrap','--organization','GitHub fixture','--username','owner','--name','Owner','--password-file',str(pw)]]:
   try:result=subprocess.run(args+command,capture_output=True,text=True,timeout=60)
   except subprocess.TimeoutExpired:raise AssertionError('GitHub fixture '+command[0]+' exceeded 60 seconds') from None
   safe_reason='STORAGE_UNAVAILABLE' if 'platform storage unavailable' in result.stderr else ('CAPACITY_UNAVAILABLE' if 'platform capacity unavailable' in result.stderr else 'OTHER_FAILURE')
   assert result.returncode==0,'GitHub fixture '+command[0]+' initialization failed: '+safe_reason
  policy=Path(folder)/'policy.json';policy.write_text(json.dumps({'gitlab_origin':'https://gitlab.com','callback_origin':base,'github_origin':origin,'github_api_origin':origin}))
  # Initial missing route RED does not depend on new operator fields.
  server=subprocess.Popen(args+['serve','--bind',f'127.0.0.1:{port}'],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
  try:
   assert server.stderr.readline().startswith('nexora-api listening'),'GitHub fixture startup failed'
   status,session,_=request('/api/v1/auth/login',{'username':'owner','password':password});assert status==200,'Fixture login failed';owner=session['access_token']
   probe='/api/v1/git/github/webhook' if mode=='webhook' else ('/api/v1/git/github/manifest/start' if mode=='manifest' else '/api/v1/git/github/configuration')
   assert request(probe,{},owner)[0]!=404,'GitHub webhook API missing' if mode=='webhook' else ('GitHub manifest API missing' if mode=='manifest' else 'GitHub App API missing')
  finally:server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,'Initial shutdown failed';server.stderr.close()
  server=subprocess.Popen(args+['serve','--bind',f'127.0.0.1:{port}','--git-policy',str(policy)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
  try:
   assert server.stderr.readline().startswith('nexora-api listening'),'GitHub policy startup failed'
   config={'app_id':'88','client_id':'fixture-client','client_secret':secret,'private_key_pem':pem,'webhook_secret':webhook_secret,'redirect_uri':base+'/api/v1/git/github/callback','enabled':True,'expected_revision':0}
   if mode=='catalog':assert request('/api/v1/git/github/configuration',token=owner)[0]==404,'Missing GitHub configuration cannot be read as metadata'
   if mode=='manifest':
    status,flow,headers=request('/api/v1/git/github/manifest/start',{'name':'Nexora fixture','expected_revision':0},owner);assert status==200,'Manifest start failed'
    manifest=flow['manifest'];params=parse_qs(urlsplit(flow['action_url']).query);assert manifest['default_permissions']=={'contents':'write','metadata':'read','pull_requests':'write','actions':'read','checks':'read','deployments':'read','administration':'read'} and manifest['hook_attributes']['url']==base+'/api/v1/git/github/webhook','Manifest did not bound permissions/callback'
    assert urlsplit(flow['action_url']).path=='/settings/apps/new','Wrong manifest registration target'
    code=secrets.token_hex(20);manifest_codes.add(code);cookie=headers['Set-Cookie'].split(';',1)[0];callback='/api/v1/git/github/manifest/callback?'+urlencode({'state':params['state'][0],'code':code})
    assert request(callback)[0]==401,'Manifest accepted missing correlation cookie'
    assert request(callback.replace('/manifest/callback','/callback'),cookie=cookie)[0]==401,'Manifest state entered user-consent protocol'
    # State and browser correlation must survive a full server restart.
    server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,'Pending manifest shutdown failed';server.stderr.close()
    server=subprocess.Popen(args+['serve','--bind',f'127.0.0.1:{port}','--git-policy',str(policy)],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
    assert server.stderr.readline().startswith('nexora-api listening'),'Manifest restart failed'
    status,metadata,_=request(callback,cookie=cookie);assert status==200 and metadata['app_id']=='88','Manifest conversion did not configure verified App'
    assert request(callback,cookie=cookie)[0]==401,'Manifest callback replay accepted'
    assert request('/api/v1/git/connections',token=owner)[1]['items']==[],'Manifest registration authorized a repository connection'
    assert all(value not in json.dumps(metadata) for value in [pem,secret,webhook_secret]),'Manifest response leaked generated secrets'
   else:
    assert request('/api/v1/git/github/configuration',{**config,'app_id':'89'},owner)[0]==403,'Configuration trusted an unverified app id'
    status,metadata,_=request('/api/v1/git/github/configuration',config,owner,idem='configure-app');assert status==201 and metadata['slug']=='nexora-fixture','Signed app configuration failed'
    assert request('/api/v1/git/github/configuration',config,owner,idem='configure-app')[1]['revision']==1,'App configuration retry changed revision'
   def begin(token=owner):
    status,flow,headers=request('/api/v1/git/github/start',{},token);assert status==200,'GitHub consent start failed'
    params={k:v[0] for k,v in parse_qs(urlsplit(flow['authorization_url']).query).items()};assert params['scope']=='offline_access' and params['code_challenge_method']=='S256','GitHub consent protocol incorrect'
    code=secrets.token_urlsafe(24);codes[code]=params;callback='/api/v1/git/github/callback?'+urlencode({'code':code,'state':params['state']});cookie=headers['Set-Cookie'].split(';',1)[0]
    return callback,cookie
   def consent():
    callback,cookie=begin()
    assert request(callback)[0]==401,'GitHub callback accepted missing cookie'
    assert request(callback.replace('/github/','/gitlab/'),cookie=cookie)[0]==401,'GitLab accepted GitHub purpose state'
    status,pending,_=request(callback,cookie=cookie);assert status==200 and pending['remote_account_id']=='42' and 'access_token' not in pending,'GitHub user consent failed'
    assert request(callback,cookie=cookie)[0]==401,'GitHub callback replay accepted'
    return pending['id']
   if mode=='catalog':
    status,saved,_=request('/api/v1/git/github/configuration',token=owner)
    assert status==200 and saved==metadata and not any(k in saved for k in ['secrets','private_key_pem','client_secret','webhook_secret']),'GitHub metadata read changed or leaked configuration'
    assert request('/api/v1/git/github/configuration')[0]==401,'Unauthenticated actor read provider configuration'
   pending=consent()
   status,actor,_=request('/api/v1/actors',{'kind':'agent','name':'Other connection manager','purpose':'Isolation fixture'},owner);assert status==201,'Second actor creation failed'
   assert request(f"/api/v1/actors/{actor['id']}/grants",{'capability':'connections:manage'},owner)[0]==204,'Second actor grant failed'
   status,issued,_=request(f"/api/v1/actors/{actor['id']}/api-keys",{'name':'Other manager','scope':{'capabilities':['connections:manage'],'project_ids':[],'all_projects':False,'installation':True},'expires_in_seconds':3600},owner);assert status==201,'Second actor key failed';other=issued['key']
   if mode=='catalog':
    assert request('/api/v1/git/github/configuration',token=other)[0]==403,'Connection manager read installation configuration'
    assert request('/api/v1/git/connections?limit=1',token=other)[1]['items']==[],'Manager saw another actor connection'
   assert request(f'/api/v1/git/github/authorizations/{pending}/installations',token=other)[0]==404,'Another actor read a pending consent'
   assert request(f'/api/v1/git/github/authorizations/{pending}/connections',{'installation_id':'7'},other)[0]==404,'Another actor claimed a pending consent'
   status,second_session,_=request('/api/v1/auth/login',{'username':'owner','password':password});assert status==200,'Secondary session failed'
   denied_callback,denied_cookie=begin(second_session['access_token'])
   assert request('/api/v1/auth/logout',{},second_session['access_token'])[0]==204,'Secondary session logout failed'
   assert request(denied_callback,cookie=denied_cookie)[0]==401,'Revoked initiating credential completed GitHub consent'
   status,choices,_=request(f'/api/v1/git/github/authorizations/{pending}/installations',token=owner);assert status==200 and [v['id'] for v in choices['items']]==['7'] and choices['installation_url']==origin+'/apps/nexora-fixture/installations/new','Installation list did not confine app identity/link'
   for bad in ['8','9']:
    assert request(f'/api/v1/git/github/authorizations/{pending}/connections',{'installation_id':bad},owner)[0] in [403,404],'Unverified installation accepted'
   status,connection,_=request(f'/api/v1/git/github/authorizations/{pending}/connections',{'installation_id':'7'},owner,idem='choose-installation');assert status==201 and connection['installation_id']=='7','Installation connection failed';cid=connection['id']
   assert request(f'/api/v1/git/github/authorizations/{pending}/connections',{'installation_id':'7'},owner,idem='choose-installation')[1]['id']==cid,'Installation selection retry duplicated connection'
   # Trigger renewal at a controlled boundary, independently of machine speed.
   from expire_github_fixture_connection import expire_github_fixture_connection
   expire_github_fixture_connection(ns,cid)
   if mode=='profile':
    from verify_repository_profiles import verify_unlinked_profile
    profile_route=verify_unlinked_profile(request,cid,owner,'github')
    assert request(profile_route,token=other)[0]==403,'Another connection owner read global profile'
    profile_control['pr_status']=403
    unavailable=request(profile_route,token=owner)
    assert unavailable[0]==200 and unavailable[1]['pull_requests_available'] is False and unavailable[1]['pull_requests']==[],'PR denial hid repository metadata'
    profile_control['pr_status']=200
    from verify_repository_profiles import verify_profile_live_authority
    verify_profile_live_authority(request,profile_route,owner,session['actor_id'],profile_control)
   if mode=='discovery':
    from verify_remote_repository_pages import verify_remote_repository_pages
    global_route=f'/api/v1/git/connections/{cid}/repositories'
    verify_remote_repository_pages(request,global_route,owner,['101','103'])
    from verify_account_discovery_authority import verify_account_discovery_authority
    verify_account_discovery_authority(request,global_route,owner,session['actor_id'],discovery_control)
    assert request(global_route,token=other)[0]==403,'Other connection owner discovered private account repositories'
   assert request('/api/v1/auth/methods')[1]['oidc']==[],'Repository consent became corporate login'
   status,project,_=request('/api/v1/projects',{'name':'GitHub project'},owner);assert status==201,'Project creation failed';pid=project['id']
   status,repo,_=request(f'/api/v1/projects/{pid}/repositories',{'provider':'github','full_name':'team/backend'},owner);assert status==201,'Repository draft failed';rid=repo['id']
   content_forbidden=True
   assert request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{'connection_id':cid,'expected_revision':1},owner)[0]==403,'Metadata alone authorized code'
   content_forbidden=False
   if mode=='webhook':
    from concurrent.futures import ThreadPoolExecutor
    pause_proof=True
    with ThreadPoolExecutor(max_workers=1) as pool:
     pending_bind=pool.submit(request,f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{'connection_id':cid,'expected_revision':1},owner)
     try:
      assert proof_entered.wait(timeout=20),'Scoped source proof did not reach gate'
      removed={'action':'removed','installation':{'id':7,'app_id':88},'repositories_removed':[{'id':101}]};raw=json.dumps(removed,separators=(',',':')).encode()
      headers={'X-GitHub-Event':'installation_repositories','X-GitHub-Delivery':'remove-during-binding','X-Hub-Signature-256':'sha256='+hmac.new(webhook_secret.encode(),raw,hashlib.sha256).hexdigest()}
      assert request('/api/v1/git/github/webhook',raw=raw,method='POST',extra_headers=headers)[0]==202,'Concurrent permission withdrawal was not accepted'
     finally:pause_proof=False;proof_release.set()
     assert pending_bind.result(timeout=30)[0]==409,'Permission withdrawal raced past final repository authorization'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['authorization_status']=='awaiting_authorization','In-flight proof retained withdrawn source authority'
   binding={'connection_id':cid,'expected_revision':1}
   if mode=='discovery':
    from verify_remote_repository_pages import verify_remote_repository_pages
    route=f'/api/v1/projects/{pid}/git/connections/{cid}/repositories'
    verify_remote_repository_pages(request,route,owner,['101','103'])
    assert request(route,token=other)[0] in [403,404],'Connection management bypassed project authority during discovery'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['authorization_status']=='awaiting_authorization','Metadata discovery granted code access'
    repository_name='moved/renamed';old_name_reused=True
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{**binding,'expected_remote_id':'00101'},owner)[0]==422,'Noncanonical remote identity accepted'
    binding['expected_remote_id']='101'
   status,bound,_=request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',binding,owner);assert status==200 and bound['remote_id']=='101' and bound['authorization_status']=='authorized','GitHub remote binding failed'
   if mode=='discovery':
    assert bound['remote_full_name']==repository_name,'Selection followed reused name instead of expected identity'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{'connection_id':cid,'expected_revision':2,'expected_remote_id':'999'},owner)[0]==409,'Bound identity accepted a different selected repository'
   if mode=='profile':
    from verify_repository_profiles import verify_linked_profile
    registered_profile=verify_linked_profile(request,cid,pid,rid,owner)
   assert installation_tokens,'No repository-scoped installation proof was requested'
   assert refresh_count==1,'Expiring GitHub user token was not refreshed exactly once'
   if mode=='rename':
    repository_name='moved/renamed';old_name_reused=True;content_forbidden=True
    route=f'/api/v1/projects/{pid}/repositories/{rid}'
    assert request(route+'/authorize',{'connection_id':cid,'expected_revision':2},owner)[0]==403,'Rename bypassed code-read proof'
    unchanged=request(route,token=owner)[1];assert unchanged['revision']==2 and unchanged['remote_full_name']=='team/backend','Rejected rename changed registered state'
    content_forbidden=False
    status,renamed,_=request(route+'/authorize',{'connection_id':cid,'expected_revision':2},owner)
    assert status==200,'Renamed GitHub repository was not resolved by stable remote identity'
    assert renamed['id']==rid and renamed['remote_id']=='101' and renamed['remote_full_name']==repository_name and renamed['requested_full_name']=='team/backend' and renamed['revision']==3,'Rename replaced registered identity or followed reused old name'
   status,listed,_=request('/api/v1/git/connections',token=owner);serialized=json.dumps(listed);assert status==200 and all(v not in serialized for v in [secret,pem,*users,*refreshes,*installation_tokens]),'GitHub credentials leaked'
   if mode=='discovery':
    from verify_remote_discovery_revocation import verify_remote_discovery_revocation
    assert request(f"/api/v1/actors/{actor['id']}/grants",{'capability':'repositories:write','project_id':pid},owner)[0]==204,'Discovery writer grant failed'
    status,issued_writer,_=request(f"/api/v1/actors/{actor['id']}/api-keys",{'name':'Discovery writer','scope':{'capabilities':['repositories:write'],'project_ids':[pid],'all_projects':False,'installation':False},'expires_in_seconds':3600},owner);assert status==201,'Discovery writer key failed';writer=issued_writer['key']
    delegation=f'/api/v1/git/connections/{cid}/projects/{pid}'
    assert request(delegation,{},owner)[0]==204,'Discovery delegation failed'
    assert request(global_route,token=writer)[0]==403,'Project delegation leaked global account catalog'
    verify_remote_discovery_revocation(request,f'/api/v1/projects/{pid}/git/connections/{cid}/repositories',owner,writer,delegation,cid,discovery_control)
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['authorization_status']=='revoked','Rejected discovery credential retained bound source'
   if mode=='state':
    from verify_provider_configuration_state import verify_provider_configuration_state
    stale_callback,stale_cookie=begin()
    provider_control['available']=False
    try:verify_provider_configuration_state(request,'/api/v1/git/github/configuration',owner,other,cid,[(pid,rid)])
    finally:provider_control['available']=True
    assert request(stale_callback,cookie=stale_cookie)[0]==401,'State change retained stale GitHub consent'
    fresh_pending=consent();status,fresh_connection,_=request(f'/api/v1/git/github/authorizations/{fresh_pending}/connections',{'installation_id':'7'},owner);assert status==201,'Re-enabled GitHub credentials could not create new connection'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{'connection_id':fresh_connection['id'],'expected_revision':3},owner)[0]==200,'Fresh source proof did not restore GitHub binding'
    assert request('/api/v1/git/github/configuration',{**config,'expected_revision':3},owner)[0]==200,'Credential replacement after state change failed'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{'connection_id':fresh_connection['id'],'expected_revision':4},owner)[0]==200,'Credential context did not survive different administrative revision'
    status,replayed,_=request('/api/v1/git/github/configuration',{'enabled':False,'expected_revision':1},owner,method='PATCH',extra_headers={'Idempotency-Key':'state-disable-once'});assert status==200 and replayed['revision']==2,'State receipt did not replay'
    assert request('/api/v1/git/github/configuration',token=owner)[1]['enabled']==True and request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['authorization_status']=='authorized','Historical state receipt revoked newly granted authority'
    assert request(f"/api/v1/git/connections/{fresh_connection['id']}",token=owner,method='DELETE')[0]==204,'Fresh GitHub state fixture cleanup failed'
   if mode=='catalog':
    from verify_git_connection_pages import verify_git_connection_pages
    other_callback,other_cookie=begin(other)
    status,other_consent,_=request(other_callback,cookie=other_cookie);assert status==200,'Other manager consent failed'
    status,other_connection,_=request(f"/api/v1/git/github/authorizations/{other_consent['id']}/connections",{'installation_id':'7'},other);assert status==201,'Other manager connection failed'
    verify_git_connection_pages(request,owner,[cid,other_connection['id']])
    verify_git_connection_pages(request,other,[other_connection['id']])
   if mode=='webhook':
    from concurrent.futures import ThreadPoolExecutor
    def event(payload,kind='push',delivery=None,signed=True):
     raw=json.dumps(payload,separators=(',',':')).encode();delivery=delivery or secrets.token_hex(16)
     headers={'X-GitHub-Event':kind,'X-GitHub-Delivery':delivery}
     if signed:headers['X-Hub-Signature-256']='sha256='+hmac.new(webhook_secret.encode(),raw,hashlib.sha256).hexdigest()
     return request('/api/v1/git/github/webhook',method='POST',raw=raw,extra_headers=headers)
    push={'ref':'refs/heads/main','after':'b'*40,'repository':{'id':101},'installation':{'id':7}}
    assert event(push,signed=False)[0]==401,'Unsigned GitHub delivery accepted'
    raw=json.dumps(push,separators=(',',':')).encode();signature='sha256='+hmac.new(webhook_secret.encode(),raw,hashlib.sha256).hexdigest();headers={'X-GitHub-Event':'push','X-GitHub-Delivery':'altered-body','X-Hub-Signature-256':signature}
    assert request('/api/v1/git/github/webhook',raw=raw+b' ',method='POST',extra_headers=headers)[0]==401,'GitHub signature accepted different raw bytes'
    assert request('/api/v1/git/github/webhook',raw=raw,method='POST',extra_headers={**headers,'X-Hub-Signature-256':'sha256='+'0'*64})[0]==401,'GitHub accepted invalid HMAC'

    status,accepted,_=event(push,delivery='push-once');assert status==202 and accepted['duplicate']==False,'Signed push not accepted'
    status,current,_=request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner);assert status==200 and current['source_generation']==1 and current['latest_source_commit']=='b'*40,'Push did not mark source generation'
    assert event(push,delivery='push-once')[1]['duplicate']==True,'Repeated GitHub delivery not deduplicated'
    assert event({**push,'after':'c'*40},delivery='push-once')[0]==409,'Delivery ID accepted different payload'
    with ThreadPoolExecutor(max_workers=2) as pool:results=list(pool.map(lambda _:event(push,delivery='push-race'),range(2)))
    assert all(v[0]==202 for v in results) and sorted(v[1]['duplicate'] for v in results)==[False,True],'Concurrent GitHub delivery duplicated effect'
    current=request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1];assert current['source_generation']==2,'Delivery replay changed generation'
    other_inst={**push,'installation':{'id':8}}
    assert event(other_inst)[0]==202,'Unrelated installation event not acknowledged'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['source_generation']==2,'Another installation affected repository'
    removed={'action':'removed','installation':{'id':7,'app_id':88},'repositories_removed':[{'id':101}]}
    assert event(removed,kind='installation_repositories')[0]==202,'Repository removal not accepted'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['authorization_status']=='revoked','Removed repository retained authority'
    revoked_user={'action':'revoked','sender':{'id':42}}
    assert event(revoked_user,kind='github_app_authorization')[0]==202,'User authorization revocation not accepted'
    assert next(c for c in request('/api/v1/git/connections',token=owner)[1]['items'] if c['id']==cid)['status']=='revoked','Revoked GitHub user connection remained active'
   elif mode!='state':
    suspended=True
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}/authorize',{'connection_id':cid,'expected_revision':2},owner)[0] in [401,403],'Suspended installation remained usable'
    assert request(f'/api/v1/projects/{pid}/repositories/{rid}',token=owner)[1]['authorization_status']=='revoked','Suspension did not invalidate bound authority'
   if mode=='profile':
    assert request(profile_route,token=owner)[0] in [401,409],'Suspended installation retained global profile'
    assert request(registered_profile,token=owner)[0]==404,'Revoked source retained registered profile'
   if mode=='catalog':
    assert request('/api/v1/git/github/configuration',{**config,'enabled':False,'expected_revision':1},owner)[0]==200,'GitHub configuration disable failed'
    status,disabled,_=request('/api/v1/git/github/configuration',token=owner)
    assert status==200 and disabled['enabled']==False and disabled['revision']==2,'Disabled GitHub configuration metadata unavailable'
  finally:server.send_signal(signal.SIGINT);assert server.wait(timeout=30)==0,'Final shutdown failed';server.stderr.close()
 provider.shutdown();provider.server_close();print(json.dumps({'passed':True,'provider':'github-app-fixture','live_provider':False}))
try:main()
except Exception as error:
 import traceback
 for frame in traceback.extract_tb(error.__traceback__):print(f'{Path(frame.filename).name}:{frame.lineno} {frame.name}',file=sys.stderr)
 print(type(error).__name__,file=sys.stderr)
 if isinstance(error,AssertionError):print(str(error),file=sys.stderr)
 print('GitHub fixture failed; credential content suppressed',file=sys.stderr);sys.exit(1)
