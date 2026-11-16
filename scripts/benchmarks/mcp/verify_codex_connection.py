# /// script
# dependencies = ["mcp==2.3.0"]
# ///
"""Verify the exact MCP entry registered in Codex and the unchanged original source scope."""
import asyncio,hashlib,json,subprocess
from pathlib import Path
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client
async def main():
 workspace=Path(__file__).resolve().parents[2];base=workspace/'.nexora/local/benchmarks/smartticket'
 server=json.loads(subprocess.check_output(['codex','mcp','get','nexora-smartticket','--json'],text=True));transport=server['transport']
 source=json.loads((base/'source-inventory.json').read_text());root=Path(source['source']);current={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file() and not p.is_symlink() and p.suffix in {'.ts','.tsx'}}
 unchanged=current=={p:v['sha256'] for p,v in source['items'].items()}
 with (base/'codex-registered-server-stderr.log').open('w') as error:
  async with stdio_client(StdioServerParameters(command=transport['command'],args=transport['args']),errlog=error) as (read,write):
   async with ClientSession(read,write,read_timeout_seconds=180) as session:
    await session.initialize();tools=await session.list_tools();result=(await session.call_tool('nexora_repository_overview',{'limit':1})).model_dump(by_alias=True)
    assert not result.get('isError'), 'Registered repository snapshot unavailable'
    value=result['structuredContent'];expected=json.loads((base/'nexora-original-index.json').read_text())['snapshot'];assert value['snapshot']==expected['id'] and value['repository']==expected['repository_id'] and value['tenant']=='smartticket-local'
    row={'codex_registered_server':server['name'],'enabled':server['enabled'],'tools':len(tools.tools),'registered_snapshot_verified':True,'original_ts_tsx_unchanged':unchanged,'source_files':len(current)}
    (base/'codex-registration-verification.json').write_text(json.dumps(row,indent=2));print(json.dumps(row))
 assert unchanged,'Original sources changed during benchmark'
asyncio.run(main())
