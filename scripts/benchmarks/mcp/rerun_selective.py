# /// script
# dependencies = ["mcp==2.3.0", "psutil>=7,<8"]
# ///
"""Repeat unchanged cases; verify both cached results and actual selective backend misses."""
import asyncio,hashlib,importlib.util,json,statistics,time
from pathlib import Path
from mcp import ClientSession
from mcp.client.stdio import stdio_client

class SelectiveComparison:
    def __init__(self):
        spec=importlib.util.spec_from_file_location('queries',Path(__file__).with_name('compare_queries.py'))
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        self.bench=module.ComparisonBenchmark(output_prefix='selective-clean-',monitor_memory=False)
        self.base=self.bench.base
        self.header=json.loads((self.base/'selective-prepare.json').read_text())
        config=json.loads((self.bench.workspace/'.nexora/local/connections.json').read_text())
        secret_dir=Path(config['secrets_directory'])
        if not secret_dir.is_absolute():secret_dir=self.bench.workspace/'.nexora/local'/secret_dir
        flags=(secret_dir/'dragonfly_flags').read_text();password=next(line.split('=',1)[1] for line in flags.splitlines() if line.startswith('--requirepass='))
        spec=importlib.util.spec_from_file_location('redis_probe',self.bench.workspace/'infra/local/redis_probe.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        self.redis=module.RedisProbe(config['host'],config['redis_port'],password)
    def diagnostic_params(self):
        params=self.bench.params('nexora');params.env={'RUST_LOG':'nexora_mcp=debug'};return params
    def query(self,operation,case):
        if operation=='definition':return 'nexora_find_symbol',{'name':case['name'],'limit':100},{'FindSymbol':{'name':case['name'],'offset':0,'limit':100}}
        if operation=='context':return 'nexora_get_source_context',{'path':case['path'],'start_line':case['line'],'line_count':5},{'Context':{'path':case['path'],'start_line':case['line'],'line_count':5}}
        return 'nexora_search_code',{'query':case['name'],'limit':100},{'Search':{'query':case['name'],'offset':0,'limit':100}}
    def cache_key(self,query):
        h=self.header;body=[h['query_version'],h['tenant'],h['repository'],h['snapshot'],h['payload_hash'],query]
        return 'nexora:query:v1:'+hashlib.sha256(json.dumps(body,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
    async def misses(self):
        rows=[]
        with (self.base/'selective-clean-misses-stderr.log').open('w') as errors:
            async with stdio_client(self.diagnostic_params(),errlog=errors) as (read,write):
                async with ClientSession(read,write,read_timeout_seconds=180) as session:
                    await session.initialize()
                    await session.call_tool('nexora_repository_overview',{'limit':1})
                    for ci,case in enumerate(self.bench.cases):
                        for operation in ['definition','context','search']:
                            name,args,typed=self.query(operation,case);key=self.cache_key(typed)
                            assert self.redis.command('EXISTS',key)=='1','Query cache key does not match the measured Rust query; refuse invalid miss labels'
                            samples=[];checks=[]
                            for iteration in range(5):
                                assert self.redis.command('DEL',key)=='1','Expected this exact measured result cache entry'
                                started=time.perf_counter();response=await session.call_tool(name,args);elapsed=(time.perf_counter()-started)*1000
                                raw,value=self.bench.decode(response);samples.append(elapsed);checks.append(self.bench.correctness('nexora',operation,value,case))
                                assert value['snapshot']==self.header['snapshot']
                                assert self.redis.command('EXISTS',key)=='1','Miss did not repopulate the exact result cache entry'
                                (self.base/f'selective-clean-miss-case-{ci}-{operation}-{iteration}.json').write_text(json.dumps(raw))
                            rows.append({'case':ci,'operation':operation,'samples_ms':samples,'correctness':checks,'all_correct':all(c['passed'] for c in checks)})
        (self.base/'selective-clean-cache-misses.json').write_text(json.dumps(rows,indent=2))
        print(json.dumps({'nexora_verified_cache_misses':{op:{'median_ms':statistics.median(v for row in rows if row['operation']==op for v in row['samples_ms']),'correct_cases':sum(row['all_correct'] for row in rows if row['operation']==op)} for op in ['definition','context','search']}}),flush=True)
    async def run(self):
        await self.bench.backend('nexora')
        await self.misses()
        await self.bench.backend('serena')

if __name__=='__main__':asyncio.run(SelectiveComparison().run())
