# /// script
# dependencies = ["mcp==2.3.0", "psutil>=7,<8"]
# ///
"""Sequential local MCP comparison; private raw data stays under .nexora/local."""
import asyncio,collections,hashlib,json,os,re,statistics,time
from pathlib import Path
import psutil
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client

class ComparisonBenchmark:
    def __init__(self,output_prefix="",monitor_memory=True):
        self.output_prefix=output_prefix
        self.monitor_memory=monitor_memory
        self.workspace=Path(__file__).resolve().parents[2]
        self.base=self.workspace/'.nexora/local/benchmarks/smartticket'
        self.root=self.base/'source'
        self.settings=json.loads((self.base/'settings.json').read_text())
        self.cases=json.loads((self.base/'cases.json').read_text())
        self.report={'schema_version':1,'samples_per_case_operation':5,'case_count':len(self.cases),'backends':{}}
    @staticmethod
    def summary(values):
        return {'median':statistics.median(values),'min':min(values),'max':max(values),'samples':values}
    @staticmethod
    def decode(result):
        value=result if isinstance(result,dict) else result.model_dump(by_alias=True)
        if value.get('isError'):
            return value,None
        text='\n'.join(c.get('text','') for c in value.get('content',[]) if c.get('type')=='text')
        try:return value,json.loads(text)
        except ValueError:return value,text
    async def monitor(self,state):
        while state['running']:
            try:
                children=psutil.Process(os.getpid()).children(recursive=True)
                state['peak']=max(state['peak'],sum(p.memory_info().rss for p in children if p.is_running()))
            except psutil.Error:pass
            await asyncio.sleep(.02)
    def params(self,backend):
        if backend=='nexora':
            return StdioServerParameters(command=str(self.workspace/'target/release/nexora'),args=['--connections',str(self.workspace/'.nexora/local/connections.json'),'--language-dir',str(self.base/'profile'),'mcp','serve','--root',str(self.root),'--tenant','benchmark-smartticket-v1'])
        return StdioServerParameters(command=self.settings['serena_command'],args=['start-mcp-server','--context','desktop-app','--mode','planning','--project',str(self.root),'--enable-web-dashboard','false','--open-web-dashboard','false','--enable-gui-log-window','false','--log-level','ERROR'])
    def expected_matches(self,query):
        result=set()
        for p in self.root.rglob('*'):
            if p.is_file() and p.suffix in {'.ts','.tsx'}:
                for number,line in enumerate(p.read_text().splitlines(),1):
                    if query in line:result.add((str(p.relative_to(self.root)),number))
        return result
    def correctness(self,backend,operation,data,case):
        if data is None:return {'passed':False,'reason':'tool_error'}
        if operation=='definition':
            items=data.get('items',[]) if backend=='nexora' else data
            if not isinstance(items,list):return {'passed':False,'reason':'unexpected_shape'}
            locations=[]
            for n in items:
                if backend=='nexora' and n.get('evidence'):
                    e=n['evidence'];locations.append((e['path'],e['span']['start']['line']))
                elif backend=='serena' and n.get('body_location'):
                    locations.append((n.get('relative_path'),n['body_location']['start_line']+1))
            return {'passed':(case['path'],case['line']) in locations,'returned_items':len(items),'exact_target_count':locations.count((case['path'],case['line']))}
        if operation=='context':
            actual='\n'.join(row['text'] for row in data.get('items',[])) if backend=='nexora' and isinstance(data,dict) else data
            return {'passed':isinstance(actual,str) and actual.rstrip('\n')==case['expected_context'].rstrip('\n')}
        expected=self.expected_matches(case['name']);actual=set()
        if backend=='nexora':
            for row in data.get('items',[]):actual.add((row['path'],row['line']))
        elif isinstance(data,dict):
            for path,rows in data.items():
                if not isinstance(rows,list):continue
                for row in rows:
                    for match in re.finditer(r'^\s*>\s*(\d+):',row,re.MULTILINE):actual.add((path,int(match.group(1))+1))
        return {'passed':actual==expected,'expected_lines':len(expected),'returned_lines':len(actual),'missing_lines':len(expected-actual),'extra_lines':len(actual-expected)}
    async def backend(self,backend):
        state={'running':True,'peak':0};monitor=asyncio.create_task(self.monitor(state)) if self.monitor_memory else None;started=time.perf_counter()
        rows=[]
        try:
            with (self.base/(self.output_prefix+backend+'-queries-stderr.log')).open('w') as errors:
                async with stdio_client(self.params(backend),errlog=errors) as (read,write):
                    async with ClientSession(read,write,read_timeout_seconds=180) as session:
                        await session.initialize();tools=await session.list_tools();startup=(time.perf_counter()-started)*1000
                        for ci,case in enumerate(self.cases):
                            for operation in ['definition','context','search']:
                                if backend=='nexora':
                                    name={'definition':'nexora_find_symbol','context':'nexora_get_source_context','search':'nexora_search_code'}[operation]
                                    args={'name':case['name'],'limit':100} if operation=='definition' else {'path':case['path'],'start_line':case['line'],'line_count':5} if operation=='context' else {'query':case['name'],'limit':100}
                                else:
                                    name={'definition':'find_symbol','context':'read_file','search':'search_for_pattern'}[operation]
                                    args={'name_path_pattern':case['name'],'max_answer_chars':100000} if operation=='definition' else {'relative_path':case['path'],'start_line':case['line']-1,'end_line':case['line']+3,'max_answer_chars':100000} if operation=='context' else {'substring_pattern':re.escape(case['name']),'restrict_search_to_code_files':True,'max_answer_chars':100000}
                                samples=[];sizes=[];checks=[]
                                # First call is recorded separately; repeat timings are not labelled fully cold.
                                for iteration in range(6):
                                    call_start=time.perf_counter();result=await session.call_tool(name,args);ms=(time.perf_counter()-call_start)*1000
                                    raw,data=self.decode(result);check=self.correctness(backend,operation,data,case)
                                    (self.base/f'{self.output_prefix}{backend}-case-{ci}-{operation}-{iteration}.json').write_text(json.dumps(raw))
                                    if iteration==0:first=ms
                                    else:samples.append(ms);sizes.append(len(json.dumps(raw).encode()));checks.append(check)
                                rows.append({'case':ci,'extension':Path(case['path']).suffix,'operation':operation,'first_call_ms':first,'warm_ms':self.summary(samples),'response_bytes':self.summary(sizes),'correctness':checks,'all_correct':all(c['passed'] for c in checks)})
                        self.report['backends'][backend]={'startup_initialize_list_ms':startup,'advertised_tool_count':len(tools.tools),'sampled_process_tree_peak_rss_bytes':state['peak'] if self.monitor_memory else None,'memory_sampling_during_timing':self.monitor_memory,'queries':rows}
        finally:
            state['running']=False
            if monitor is not None:await monitor
        (self.base/(self.output_prefix+'query-comparison.json')).write_text(json.dumps(self.report,indent=2))
        aggregate={op:{'median_ms':statistics.median([sample for row in rows if row['operation']==op for sample in row['warm_ms']['samples']]),'correct_cases':sum(row['all_correct'] for row in rows if row['operation']==op),'cases':len(self.cases)} for op in ['definition','context','search']}
        print(json.dumps({'backend':backend,'startup_ms':startup,'operations':aggregate}),flush=True)
    async def run(self):
        for backend in ['nexora','serena']:await self.backend(backend)

if __name__=='__main__':
    asyncio.run(ComparisonBenchmark().run())
