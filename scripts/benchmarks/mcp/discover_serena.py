# /// script
# dependencies = ["mcp==2.3.0"]
# ///
"""Discover isolated installed Serena tool schemas; write private local metadata only."""
import asyncio,json
from pathlib import Path
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client
async def main():
    base=Path('.nexora/local/benchmarks/smartticket').resolve()
    params=StdioServerParameters(command=json.loads((base/'settings.json').read_text())['serena_command'],args=['start-mcp-server','--context','desktop-app','--mode','planning','--project',str(base/'source'),'--enable-web-dashboard','false','--open-web-dashboard','false','--enable-gui-log-window','false','--log-level','ERROR'])
    with (base/'serena-mcp.log').open('w') as errors:
        async with stdio_client(params,errlog=errors) as (read,write):
            async with ClientSession(read,write,read_timeout_seconds=180) as session:
                init=await session.initialize();tools=await session.list_tools()
                data=tools.model_dump(by_alias=True)
                (base/'serena-tools.json').write_text(json.dumps(data,indent=2))
                print(json.dumps({'serena_name':init.model_dump(by_alias=True)['serverInfo']['name'],'tools':[t.name for t in tools.tools]}))
asyncio.run(main())
