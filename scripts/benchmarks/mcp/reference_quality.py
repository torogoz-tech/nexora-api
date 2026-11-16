# /// script
# dependencies = ["mcp==2.3.0", "psutil>=7,<8", "pyyaml>=6,<7"]
# ///
"""Independent public ground truth: recursion, imports, alias, JSX and parameter shadowing."""
import asyncio,importlib.util,json,shutil,subprocess,time
from pathlib import Path
import yaml
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client

spec=importlib.util.spec_from_file_location('queries',Path(__file__).with_name('compare_queries.py'));module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);bench=module.ComparisonBenchmark()
root=bench.base/'reference-source';shutil.copytree(bench.workspace/'tests/fixtures/mcp-comparison',root,dirs_exist_ok=True)
(root/'tsconfig.json').write_text(json.dumps({'compilerOptions':{'jsx':'preserve','moduleResolution':'node','noEmit':True},'include':['*.ts','*.tsx']}))
serena=bench.settings['serena_command']
if not (root/'.serena/project.yml').exists():
 subprocess.run([serena,'project','create',str(root),'--name','nexora-reference-quality','--ls','typescript'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
p=root/'.serena/project.yml';c=yaml.safe_load(p.read_text());c.update(read_only=True,activation_command=None,agent_interface='tools');p.write_text(yaml.safe_dump(c,sort_keys=False))
with (bench.base/'reference-nexora-index.json').open('w') as output,(bench.base/'reference-nexora-index.log').open('w') as error:
 subprocess.run([str(bench.workspace/'target/release/nexora'),'--language-dir',str(bench.base/'profile'),'index',str(root),'--tenant','benchmark-references','--report'],stdout=output,stderr=error,check=True)

async def main():
 report={'ground_truth':{'callers':['target','directCaller','aliasCaller','Card'],'excluded_shadow':'shadowCaller','source':'tests/fixtures/mcp-comparison'},'backends':{}}
 for backend in ['nexora','serena']:
  params=bench.params(backend);args=list(params.args)
  args[args.index('--root' if backend=='nexora' else '--project')+1]=str(root)
  if backend=='nexora':args[args.index('--tenant')+1]='benchmark-references'
  with (bench.base/f'references-{backend}-stderr.log').open('w') as error:
   async with stdio_client(StdioServerParameters(command=params.command,args=args),errlog=error) as (read,write):
    async with ClientSession(read,write,read_timeout_seconds=180) as session:
     await session.initialize()
     if backend=='nexora':
      _,found=bench.decode(await session.call_tool('nexora_find_symbol',{'name':'target','limit':100}))
      target=next(n for n in found['items'] if n['kind']=='Function' and n.get('evidence',{}).get('path')=='target.ts')
      raw,refs=bench.decode(await session.call_tool('nexora_find_references',{'node':target['id'],'limit':100}))
      snapshot=json.loads((bench.base/'reference-nexora-index.json').read_text())['snapshot'];nodes={n['id']:n for n in snapshot['graph']['nodes']};reported=[]
      for edge in refs['items']:
       if edge['kind']=='CALLS' and edge['resolution']=='Resolved':
        reported.append({'source_name':nodes.get(edge['source'],{}).get('name'),'path':edge['evidence']['path'],'line':edge['evidence']['span']['start']['line']})
      report['backends'][backend]={'reported_resolved_calls':reported,'unresolved_call_edges':sum(e['kind']=='CALLS' and e['resolution']!='Resolved' for e in snapshot['graph']['edges'])}
     else:
      raw,refs=bench.decode(await session.call_tool('find_referencing_symbols',{'name_path':'target','relative_path':'target.ts','max_answer_chars':100000}))
      report['backends'][backend]={'raw_referencing_symbols':refs}
     (bench.base/f'references-{backend}-raw.json').write_text(json.dumps(raw,indent=2))
 (bench.base/'reference-quality.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
asyncio.run(main())
