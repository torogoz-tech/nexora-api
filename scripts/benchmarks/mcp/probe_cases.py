# /// script
# dependencies = ["mcp==2.3.0"]
# ///
import asyncio,json
from pathlib import Path
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client
async def main():
 base=Path('.nexora/local/benchmarks/smartticket').resolve();case=json.loads((base/'cases.json').read_text())[0]
 commands=[('nexora',str(Path.cwd()/'target/release/nexora'),['--connections',str(Path.cwd()/'.nexora/local/connections.json'),'--language-dir',str(base/'profile'),'mcp','serve','--root',str(base/'source'),'--tenant','benchmark-smartticket-v1']),('serena',json.loads((base/'settings.json').read_text())['serena_command'],['start-mcp-server','--context','desktop-app','--mode','planning','--project',str(base/'source'),'--enable-web-dashboard','false','--open-web-dashboard','false','--enable-gui-log-window','false','--log-level','ERROR'])]
 for backend,command,args in commands:
  with (base/(backend+'-probe-stderr.log')).open('w') as error:
   async with stdio_client(StdioServerParameters(command=command,args=args),errlog=error) as (read,write):
    async with ClientSession(read,write,read_timeout_seconds=180) as session:
     await session.initialize()
     name='nexora_find_symbol' if backend=='nexora' else 'find_symbol';params={'name':case['name']} if backend=='nexora' else {'name_path_pattern':case['name']}
     found=(await session.call_tool(name,params)).model_dump(by_alias=True)
     name='nexora_get_source_context' if backend=='nexora' else 'read_file';params={'path':case['path'],'start_line':case['line'],'line_count':5} if backend=='nexora' else {'relative_path':case['path'],'start_line':case['line']-1,'end_line':case['line']+3}
     context=(await session.call_tool(name,params)).model_dump(by_alias=True)
     (base/(backend+'-probe.json')).write_text(json.dumps({'found':found,'context':context},indent=2))
     texts=[c.get('text','') for c in found.get('content',[])];value=json.loads(texts[0]) if texts else None
     print(json.dumps({'backend':backend,'error':found.get('isError'),'decoded_type':type(value).__name__,'decoded_first_keys':list(value[0]) if isinstance(value,list) and value else list(value) if isinstance(value,dict) else [],'context_is_error':context.get('isError')}))
asyncio.run(main())
