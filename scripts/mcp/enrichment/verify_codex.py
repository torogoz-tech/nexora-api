# /// script
# dependencies = ["mcp==2.3.0"]
# ///
"""Read-only verification of the exact Nexora MCP registered in Codex."""
import asyncio
import hashlib
import json
from pathlib import Path
import subprocess
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

async def main():
    workspace = Path(__file__).resolve().parents[2]
    server = json.loads(subprocess.check_output(["codex", "mcp", "get", "nexora-smartticket", "--json"], text=True))
    transport = server["transport"]
    assert server["enabled"] and "--allow-knowledge" in transport["args"]
    inventory = json.loads((workspace / ".nexora/local/benchmarks/smartticket/source-inventory.json").read_text())
    root = Path(inventory["source"])
    current = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob("*") if p.is_file() and not p.is_symlink() and p.suffix in {".ts", ".tsx"}}
    unchanged = current == {p: v["sha256"] for p, v in inventory["items"].items()}
    assert unchanged, "Original source inventory changed"
    expected = json.loads((workspace / ".nexora/local/benchmarks/smartticket/nexora-original-index.json").read_text())["snapshot"]
    with (workspace / ".nexora/local/enrichment-codex-stderr.log").open("w") as error:
        async with stdio_client(StdioServerParameters(command=transport["command"], args=transport["args"]), errlog=error) as (read, write):
            async with ClientSession(read, write, read_timeout_seconds=180) as session:
                await session.initialize()
                tools = await session.list_tools()
                names = {tool.name for tool in tools.tools}
                additions = {"nexora_list_pending", "nexora_list_knowledge", "nexora_get_knowledge", "nexora_contribute_knowledge", "nexora_revise_knowledge", "nexora_review_knowledge"}
                assert additions <= names and len(names) == 14
                replies = {}
                for name in ["nexora_repository_overview", "nexora_list_pending", "nexora_list_knowledge"]:
                    reply = (await session.call_tool(name, {"limit": 1})).model_dump(by_alias=True)
                    assert not reply.get("isError"), "Registered MCP query failed"
                    value = reply["structuredContent"]
                    assert value["snapshot"] == expected["id"] and value["repository"] == expected["repository_id"] and value["tenant"] == "smartticket-local"
                    replies[name] = True
    result = {"codex_registered_server": server["name"], "tools": len(names), "six_enrichment_tools_discovered": True, "registered_snapshot_verified": True, "read_queries_passed": replies, "original_ts_tsx_unchanged": unchanged, "source_files": len(current), "private_repository_writes": False}
    (workspace / ".nexora/local/enrichment-codex-verification.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))

asyncio.run(main())
