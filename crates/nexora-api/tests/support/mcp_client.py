# /// script
# dependencies = ["mcp==2.3.0"]
# ///
"""Real SDK against the local API executable; generated secrets never logged."""
import asyncio
import json
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client
import httpx2

async def verify(address, keys, projects):
    for index, key in enumerate(keys):
        async with httpx2.AsyncClient(headers={"Authorization": "Bearer " + key}, trust_env=False, timeout=15) as http:
            async with streamable_http_client(address + "/mcp", http_client=http) as (read, write):
                async with ClientSession(read, write, read_timeout_seconds=15) as client:
                    initialized = await client.initialize()
                    assert initialized.model_dump(by_alias=True)["serverInfo"]["name"] == "nexora", "Wrong server identity"
                    catalog = await client.list_tools()
                    assert {t.name for t in catalog.tools} == {
                        "nexora_projects_list", "nexora_project_get", "nexora_repositories_list",
                        "nexora_repository_get", "nexora_repository_register", "nexora_repository_change",
                        "nexora_repository_overview", "nexora_find_symbol", "nexora_find_references",
                        "nexora_get_dependencies", "nexora_traverse_graph", "nexora_search_code",
                        "nexora_get_source_context",
                        "nexora_list_pending", "nexora_list_knowledge", "nexora_get_knowledge",
                        "nexora_contribute_knowledge", "nexora_revise_knowledge", "nexora_review_knowledge",
                        "nexora_sync_repository", "nexora_index_repository", "nexora_repository_job_get", "nexora_repository_job_cancel",
                        "nexora_workflows_list", "nexora_workflow_get", "nexora_workflow_create",
                        "nexora_workflow_change", "nexora_workflow_publish",
                        "nexora_workflow_version_get", "nexora_workflow_versions_list",
                        "nexora_task_type_create", "nexora_backlog_create", "nexora_backlog_change",
                        "nexora_backlog_reorder", "nexora_task_create", "nexora_task_change",
                        "nexora_task_transition", "nexora_task_get", "nexora_backlog_get",
                        "nexora_tasks_list", "nexora_task_history", "nexora_task_approval_get",
                        "nexora_task_dependency_create", "nexora_task_dependency_resolve", "nexora_task_dependency_remove",
                        "nexora_task_dependencies_list", "nexora_task_comment_add", "nexora_task_comments_list",
                        "nexora_task_template_create", "nexora_task_template_change", "nexora_task_template_publish",
                        "nexora_task_template_apply", "nexora_task_template_get", "nexora_task_template_version_get",
                        "nexora_task_templates_list", "nexora_task_template_versions_list",
                        "nexora_document_create", "nexora_document_change", "nexora_document_revise", "nexora_document_transition", "nexora_document_publish", "nexora_document_get", "nexora_document_version_get", "nexora_document_versions_list", "nexora_document_export",
                        "nexora_documents_list", "nexora_document_history", "nexora_document_publications_list",
                        "nexora_context_manifest_create", "nexora_context_manifest_get", "nexora_context_manifest_status",
                        "nexora_document_link_create", "nexora_document_link_remove", "nexora_context_package_create", "nexora_document_links_list", "nexora_task_documents_list", "nexora_context_package_get", "nexora_context_package_export",
                        "nexora_approval_request_create", "nexora_approval_request_get", "nexora_approval_requests_list", "nexora_approval_request_cancel",
                        "nexora_document_approval_get", "nexora_task_types_list", "nexora_backlogs_list", "nexora_task_type_change",
                        "nexora_task_move", "nexora_task_workflow_migrate",
                    }, "Unexpected platform catalog"
                    described = {tool.name: tool.model_dump(by_alias=True) for tool in catalog.tools}
                    assert "$defs" not in described["nexora_task_get"]["inputSchema"], "Read lookup should not ship unreferenced input definitions"
                    def local_references(value):
                        if isinstance(value, dict):
                            ref = value.get("$ref", "")
                            if ref.startswith("#/$defs/"):
                                yield ref.removeprefix("#/$defs/")
                            for nested in value.values():
                                yield from local_references(nested)
                        elif isinstance(value, list):
                            for nested in value:
                                yield from local_references(nested)
                    for tool_name, tool in described.items():
                        if not tool_name.startswith(("nexora_task", "nexora_backlog", "nexora_document", "nexora_context", "nexora_approval")):
                            continue
                        schema = dict(tool["inputSchema"])
                        definitions = schema.pop("$defs", {})
                        pending = list(local_references(schema))
                        reachable = set()
                        while pending:
                            definition = pending.pop()
                            if definition in reachable:
                                continue
                            assert definition in definitions, "Unresolved local tool reference"
                            reachable.add(definition)
                            pending.extend(local_references(definitions[definition]))
                        assert set(definitions) == reachable, "Tool shipped unrelated definitions"
                    reply = (await client.call_tool("nexora_projects_list", {"limit": 1})).model_dump(by_alias=True)
                    assert not reply["isError"], "Authorized list failed"
                    assert reply["structuredContent"]["items"][0]["id"] == projects[index], "Project isolation failed"
                    arguments = {"project_id": projects[index], "registration": {"provider": "github", "full_name": "fixture/repository"}, "request_id": "sdk-repository-once"}
                    registered = (await client.call_tool("nexora_repository_register", arguments)).model_dump(by_alias=True)
                    assert not registered["isError"], "Repository registration failed"
                    repository = registered["structuredContent"]
                    assert repository["authorization_status"] == "awaiting_authorization", "Registration implied provider authorization"
                    replay = (await client.call_tool("nexora_repository_register", arguments)).model_dump(by_alias=True)
                    assert replay["structuredContent"]["id"] == repository["id"], "SDK retry duplicated inscription"
                    lookup = {"project_id": projects[index], "repository_id": repository["id"]}
                    code = (await client.call_tool("nexora_repository_overview", lookup)).model_dump(by_alias=True)
                    assert code["isError"], "Metadata key or unverified inscription granted code access"
                    fetched = (await client.call_tool("nexora_repository_get", lookup)).model_dump(by_alias=True)
                    assert fetched["structuredContent"]["id"] == repository["id"], "Repository lookup failed"
                    page = (await client.call_tool("nexora_repositories_list", {"project_id": projects[index], "limit": 1})).model_dump(by_alias=True)
                    assert len(page["structuredContent"]["items"]) == 1, "Repository list failed"
                    changed = (await client.call_tool("nexora_repository_change", {**lookup, "request_id": "sdk-archive-once", "change": {"expected_revision": 1, "archived": True}})).model_dump(by_alias=True)
                    assert changed["structuredContent"]["revision"] == 2, "Repository archive failed"
                    denied = (await client.call_tool("nexora_repository_register", {**arguments, "project_id": projects[1-index]})).model_dump(by_alias=True)
                    assert denied["isError"], "Cross-project mutation was allowed"
                    other = (await client.call_tool("nexora_project_get", {"project_id": projects[1-index]})).model_dump(by_alias=True)
                    assert other["isError"], "Cross-project access was allowed"

def main():
    binary = str(Path(sys.argv[1]).resolve())
    workspace = Path(__file__).resolve().parents[4]
    namespace = "nexora_platform_sdk_" + str(time.time_ns())
    args = [binary, "--connections", str(workspace / ".nexora/local/connections.json"), "--schema", namespace]
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryDirectory(prefix="nexora-sdk-") as folder:
        password = secrets.token_urlsafe(32)
        path = Path(folder) / "owner-password"
        path.write_text(password)
        path.chmod(0o600)
        for command in [["migrate"], ["bootstrap", "--organization", "SDK fixture", "--username", "owner", "--name", "Owner", "--password-file", str(path)]]:
            result = subprocess.run(args + command, capture_output=True, text=True, timeout=60)
            safe_reason = "STORAGE_UNAVAILABLE" if "platform storage unavailable" in result.stderr else ("CAPACITY_UNAVAILABLE" if "platform capacity unavailable" in result.stderr else "OTHER_FAILURE")
            assert result.returncode == 0, f"SDK {command[0]} initialization failed: {safe_reason}"
        server = subprocess.Popen(args + ["serve", "--bind", "127.0.0.1:0"], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        try:
            banner = server.stderr.readline().strip()
            assert banner.startswith("nexora-api listening on 127.0.0.1:"), "SDK server startup failed"
            address = "http://" + banner.split(" on ", 1)[1]
            def post(route, body, credential=None):
                headers = {"Content-Type": "application/json"}
                if credential:
                    headers["Authorization"] = "Bearer " + credential
                request = urllib.request.Request(address + route, data=json.dumps(body).encode(), headers=headers, method="POST")
                with opener.open(request, timeout=15) as reply:
                    return json.load(reply) if reply.status != 204 else None
            owner = post("/api/v1/auth/login", {"username": "owner", "password": password})["access_token"]
            projects, keys = [], []
            for name in ["SDK Project A", "SDK Project B"]:
                project = post("/api/v1/projects", {"name": name}, owner)["id"]
                actor = post("/api/v1/actors", {"kind": "agent", "name": name, "purpose": "SDK fixture"}, owner)["id"]
                for capability in ["projects:read", "repositories:read", "repositories:write"]:
                    post(f"/api/v1/actors/{actor}/grants", {"capability": capability, "project_id": project}, owner)
                key = post(f"/api/v1/actors/{actor}/api-keys", {"name": "SDK credential", "scope": {"capabilities": ["projects:read", "repositories:read", "repositories:write"], "project_ids": [project], "all_projects": False, "installation": False}, "expires_in_seconds": 3600}, owner)["key"]
                projects.append(project)
                keys.append(key)
            asyncio.run(verify(address, keys, projects))
        finally:
            server.send_signal(signal.SIGINT)
            assert server.wait(timeout=30) == 0, "SDK server shutdown failed"
            server.stderr.close()
    print(json.dumps({"sdk": "mcp==2.3.0", "transport": "streamable-http-json", "actors": 2, "project_isolation": True, "passed": True}))

try:
    main()
except Exception as error:
    import traceback
    def describe(failure):
        print(type(failure).__name__, file=sys.stderr)
        for frame in traceback.extract_tb(failure.__traceback__):
            print(f"{Path(frame.filename).name}:{frame.lineno} in {frame.name}", file=sys.stderr)
        if isinstance(failure, AssertionError):
            print(str(failure), file=sys.stderr)
        for nested in getattr(failure, "exceptions", []):
            describe(nested)
    describe(error)
    print("MCP SDK acceptance failed; credentials and request content suppressed", file=sys.stderr)
    sys.exit(1)
