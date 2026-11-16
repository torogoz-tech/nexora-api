# /// script
# dependencies = ["mcp==2.3.0", "psutil>=7,<8"]
# ///
"""Public, independently specified graph investigations: layers, cycles, fanout and uncertainty."""
import asyncio,hashlib,importlib.util,json,shutil,subprocess,time
from pathlib import Path
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client

class ComplexityBenchmark:
    def __init__(self):
        self.workspace=Path(__file__).resolve().parents[2];self.base=self.workspace/'.nexora/local/benchmarks/complexity';self.base.mkdir(parents=True,exist_ok=True);self.root=self.base/'source';self.root.mkdir(exist_ok=True)
        self.profile=self.workspace/'.nexora/local/profiles/smartticket-ts-tsx'
        self.files={
          'flow/Checkout.tsx':"import { api } from './api';\nexport function Checkout() { return <button>{api()}</button>; }\n",
          'flow/api.ts':"import { order } from './services/order';\nexport function api() { return order(); }\n",
          'flow/services/order.ts':"import { normalize } from '../domain/normalize';\nexport function order() { return normalize(); }\n",
          'flow/domain/normalize.ts':"import { cycleA } from '../support/a';\nexport function normalize() { return cycleA(); }\n",
          'flow/support/a.ts':"import { cycleB } from './b';\nexport function cycleA() { return cycleB(); }\n",
          'flow/support/b.ts':"import { cycleA } from './a';\nexport function cycleB() { return cycleA(); }\n",
          'flow/ambiguous.ts':"import { candidate } from './candidates';\nexport function ambiguousConsumer() { return candidate(); }\n",
          'flow/candidates.ts':"export function candidate() { return 1; }\n",
          'flow/candidates/index.ts':"export function candidate() { return 2; }\n",
          'flow/external.ts':"import { useState } from 'react';\nexport function externalConsumer() { return useState(0); }\n",
          'fanout/shared.ts':"export function shared() { return 1; }\n",
        }
        root=[]
        for i in range(12):
            self.files[f'fanout/leaves/leaf{i}.ts']=f"import {{ shared }} from '../shared';\nexport function leaf{i}() {{ return shared(); }}\n";root.append(f"import {{ leaf{i} }} from './leaves/leaf{i}';")
        self.files['fanout/root.ts']='\n'.join(root)+"\nexport function fanoutRoot() { return 1; }\n"
        for p in (self.workspace/'tests/fixtures/mcp-comparison').glob('*'):
            if p.suffix in {'.ts','.tsx'}:self.files['refs/'+p.name]=p.read_text()
        for path,text in self.files.items():p=self.root/path;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)
        self.fingerprint=hashlib.sha256(json.dumps(self.files,sort_keys=True).encode()).hexdigest()
        self.report={'public_source_files':len(self.files),'source_bytes':sum(len(s.encode()) for s in self.files.values()),'fixture_fingerprint':self.fingerprint,'scenarios':[]}
    async def call(self,name,args):
        result=await self.session.call_tool(name,dict(args,snapshot=self.snapshot));raw=result.model_dump(by_alias=True)
        if raw.get('isError'):raise RuntimeError('Complexity query failed; inspect private server log')
        value=raw.get('structuredContent') or json.loads(raw['content'][0]['text']);assert value['snapshot']==self.snapshot;return value
    async def traverse(self,path,limit=100,depth=32):return await self.call('nexora_traverse_graph',{'node':'file:'+path,'depth':depth,'limit':limit})
    @staticmethod
    def paths(result):return {n['name'] for n in result['nodes'] if n['kind']=='File'}
    async def measure(self,name,operation,check):
        samples=[];correct=[];values=[]
        for iteration in range(6):
            start=time.perf_counter();value=await operation();elapsed=(time.perf_counter()-start)*1000;passed=check(value)
            if iteration==0:first=elapsed
            else:samples.append(elapsed);correct.append(passed)
            values.append(value)
        row={'scenario':name,'first_ms':first,'warm_samples_ms':samples,'correct_runs':sum(correct),'runs':len(correct),'passed':all(correct)};self.report['scenarios'].append(row)
        (self.base/(name+'-raw.json')).write_text(json.dumps(values,indent=2));print(json.dumps(row),flush=True)
    async def reverse_impact(self):
        pending=['file:flow/domain/normalize.ts'];visited=set(pending);result=set()
        while pending:
            node=pending.pop();page=await self.call('nexora_find_references',{'node':node,'limit':100})
            assert not page['truncated']
            for edge in page['items']:
                source=edge['source']
                if edge['kind'] in {'IMPORTS','DEPENDS_ON'} and edge['resolution']=='Resolved' and source.startswith('file:') and source not in visited:
                    result.add(source[5:]);visited.add(source);pending.append(source)
        return sorted(result)
    async def references(self):
        found=await self.call('nexora_find_symbol',{'name':'target','limit':100});target=next(n for n in found['items'] if n['kind']=='Function' and n.get('evidence',{}).get('path')=='refs/target.ts')
        refs=await self.call('nexora_find_references',{'node':target['id'],'limit':100})
        return {'confirmed_reference_sites':sorted({(e['evidence']['path'],e['evidence']['span']['start']['line']) for e in refs['items'] if e['kind'] in {'CALLS','REFERENCES'} and e['resolution']=='Resolved'}),'expected_reference_sites':[['refs/target.ts',3],['refs/direct.ts',3],['refs/alias.ts',3],['refs/card.tsx',3]]}
    async def run(self):
        command=[str(self.workspace/'target/release/nexora'),'--language-dir',str(self.profile),'index',str(self.root),'--tenant','public-complexity-v1','--report']
        with (self.base/'index.json').open('w') as output,(self.base/'index-stderr.log').open('w') as errors:subprocess.run(command,stdout=output,stderr=errors,check=True)
        indexed=json.loads((self.base/'index.json').read_text());self.snapshot=indexed['snapshot']['id'];self.report['indexer_ms']=indexed['timings']['total_ms'];self.report['published_nodes']=len(indexed['snapshot']['graph']['nodes']);self.report['published_edges']=len(indexed['snapshot']['graph']['edges'])
        args=['--connections',str(self.workspace/'.nexora/local/connections.json'),'--language-dir',str(self.profile),'mcp','serve','--root',str(self.root),'--tenant','public-complexity-v1']
        with (self.base/'server-stderr.log').open('w') as errors:
            async with stdio_client(StdioServerParameters(command=str(self.workspace/'target/release/nexora'),args=args),errlog=errors) as (read,write):
                async with ClientSession(read,write,read_timeout_seconds=180) as self.session:
                    await self.session.initialize()
                    expected={'flow/Checkout.tsx','flow/api.ts','flow/services/order.ts','flow/domain/normalize.ts','flow/support/a.ts','flow/support/b.ts'}
                    await self.measure('layered_dependencies_with_cycle',lambda:self.traverse('flow/Checkout.tsx'),lambda v:self.paths(v)==expected and not v['truncated'])
                    await self.measure('transitive_reverse_module_impact',self.reverse_impact,lambda v:set(v)=={'flow/Checkout.tsx','flow/api.ts','flow/services/order.ts'})
                    await self.measure('cycle_termination',lambda:self.traverse('flow/support/a.ts'),lambda v:self.paths(v)=={'flow/support/a.ts','flow/support/b.ts'} and not v['truncated'])
                    await self.measure('fanout_and_shared_dependency',lambda:self.traverse('fanout/root.ts'),lambda v:self.paths(v)=={'fanout/root.ts','fanout/shared.ts'}|{f'fanout/leaves/leaf{i}.ts' for i in range(12)} and not v['truncated'])
                    await self.measure('bounded_traversal_reports_truncation',lambda:self.traverse('fanout/root.ts',limit=5),lambda v:v['truncated'] and len(v['nodes'])<=5)
                    await self.measure('ambiguous_import_is_preserved',lambda:self.call('nexora_get_dependencies',{'node':'file:flow/ambiguous.ts','limit':100}),lambda v:v['total']==2 and all(e['resolution']=='Ambiguous' for e in v['items']))
                    await self.measure('external_dependency_is_unresolved',lambda:self.call('nexora_get_dependencies',{'node':'file:flow/external.ts','limit':100}),lambda v:v['total']==2 and all(e['resolution']=='Unresolved' for e in v['items']))
                    await self.measure('function_alias_jsx_reference_recall',self.references,lambda v:set(map(tuple,v['confirmed_reference_sites']))==set(map(tuple,v['expected_reference_sites'])))
        (self.base/'results.json').write_text(json.dumps(self.report,indent=2));print(json.dumps({'scenarios_passed':sum(s['passed'] for s in self.report['scenarios']),'scenarios':len(self.report['scenarios'])}),flush=True)
        return all(s['passed'] for s in self.report['scenarios'])

if __name__=='__main__':raise SystemExit(0 if asyncio.run(ComplexityBenchmark().run()) else 1)
