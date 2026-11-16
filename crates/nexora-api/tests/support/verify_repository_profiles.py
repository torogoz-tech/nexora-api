"""Assertions shared by isolated GitHub and GitLab profile acceptance fixtures."""
from urllib.parse import urlencode

def verify_unlinked_profile(request, connection, owner, provider):
 route=f'/api/v1/git/connections/{connection}/repositories/101/profile'
 query='?'+urlencode({'full_name':'team/backend'})
 assert request(route+query)[0]==401,'Anonymous repository profile accepted'
 status,profile,_=request(route+query,token=owner)
 assert status==200,'Unlinked global repository profile unavailable'
 assert profile['repository']['remote_id']=='101' and profile['repository']['name']=='team/backend' and profile['repository']['provider']==provider,'Profile identity was not provider verified'
 assert profile['repository']['description']=='First line\nSecond line' and profile['repository']['default_branch']=='main','Provider display metadata missing'
 assert profile['repository']['id'] is None and profile['projects']==[],'Unlinked repository invented a project'
 assert profile['activity']==[] and profile['contributions']==[] and profile['priority_tasks']==[] and profile['counts']['tasks']==0,'Unlinked repository fabricated activity'
 assert profile['pull_requests_available'] is True and profile['counts']['pull_requests']==1 and profile['pull_requests'][0]['title']=='Review backend change','Provider pull request was not projected'
 assert request(route+'?full_name=team/frontend',token=owner)[0]==404,'Locator replaced stable repository identity'
 assert request(route+'?full_name=../backend',token=owner)[0]==422,'Invalid profile locator accepted'
 assert request(route+'?full_name=team/backend&year=1999',token=owner)[0]==422,'Invalid contribution year accepted'
 assert request(route,token=owner)[0]==422,'Missing locator accepted'
 outside=f'/api/v1/git/connections/{connection}/repositories/103/profile?full_name=team/metadata-only'
 assert request(outside,token=owner)[0] in [403,404],'Metadata-only repository became readable profile source'
 return route+query

def verify_linked_profile(request, connection, project, repository, owner):
 registered=f'/api/v1/projects/{project}/repositories/{repository}/profile'
 status,profile,_=request(registered,token=owner)
 assert status==200 and profile['repository']['id']==repository,'Registered repository profile missing'
 assert profile['projects']==[{'id':project,'name':profile['projects'][0]['name'],'repository_id':repository}],'Registered repository profile linked another project'
 assert profile['pull_requests_available'] is True and profile['counts']['pull_requests']==1,'Registered profile omitted readable provider pull requests'
 status,remote,_=request(f'/api/v1/git/connections/{connection}/repositories/101/profile?full_name=team/backend',token=owner)
 assert status==200 and remote['projects']==profile['projects'],'Global profile omitted authorized project link'
 assert remote['counts']['tasks']==0 and remote['activity']==[],'Unrelated project activity was attributed to repository'
 return registered


def verify_profile_live_authority(request, route, owner, owner_id, control):
 from concurrent.futures import ThreadPoolExecutor
 status,issued,_=request(f'/api/v1/actors/{owner_id}/api-keys',{'name':'Profile live authority','scope':{'capabilities':['connections:manage'],'project_ids':[],'all_projects':False,'installation':True},'expires_in_seconds':3600},owner)
 assert status==201,'Profile key fixture setup failed'
 control['entered'].clear();control['release'].clear();control['pause']=True
 with ThreadPoolExecutor(max_workers=1) as pool:
  pending=pool.submit(request,route,token=issued['key'])
  try:
   assert control['entered'].wait(timeout=20),'Profile did not reach provider gate'
   assert request('/api/v1/api-keys/'+issued['metadata']['id'],token=owner,method='DELETE')[0]==204,'Profile key revocation failed'
  finally:control['pause']=False;control['release'].set()
  status,body,_=pending.result(timeout=30)
  assert status==401 and 'repository' not in body,'Profile leaked metadata after credential revocation during provider IO'
