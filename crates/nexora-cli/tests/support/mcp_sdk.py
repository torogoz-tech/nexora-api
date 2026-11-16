# /// script
# dependencies = ["mcp==2.3.0"]
# ///
"""Interoperability with the official Python MCP client, using only public fixtures."""
import asyncio
import importlib.metadata
import json
from pathlib import Path
import sys
import tempfile
import time
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

async def main():
    binary = Path(sys.argv[1]).resolve()
    workspace = Path(__file__).resolve().parents[4]
    with tempfile.TemporaryDirectory(prefix="nexora-mcp-sdk-") as temp:
        root = Path(temp).resolve()
        (root / "sample.js").write_text("function sdkHello() {}\n", encoding="utf-8")
        params = StdioServerParameters(command=str(binary), args=["--connections", str(workspace / ".nexora/local/connections.json"), "--language-dir", str(workspace / "languages/javascript"), "mcp", "serve", "--root", str(root), "--tenant", "public-sdk-" + str(time.time_ns()), "--allow-index"])
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write, read_timeout_seconds=180) as session:
                initialized = await session.initialize()
                tools = await session.list_tools()
                assert len(tools.tools) == 11
                indexed = (await session.call_tool("nexora_index_repository", {})).model_dump(by_alias=True)
                assert not indexed.get("isError"), indexed
                facts = indexed["structuredContent"]
                found = (await session.call_tool("nexora_find_symbol", {"name": "sdkHello", "snapshot": facts["snapshot"]})).model_dump(by_alias=True)
                assert not found.get("isError"), found
                assert len(found["structuredContent"]["items"]) == 1
                assert found["structuredContent"] == json.loads(found["content"][0]["text"])
                await session.send_ping()
                print(json.dumps({"official_sdk": importlib.metadata.version("mcp"), "protocol": initialized.model_dump(by_alias=True)["protocolVersion"], "tools": len(tools.tools), "initialize_list_call_ping": "passed"}))

asyncio.run(main())
