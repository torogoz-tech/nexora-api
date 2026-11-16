#[path = "../../nexora-platform-store/tests/support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
#[ignore = "requires local databases; authenticated MCP reads local sources, preserves knowledge and rejects remote operator inputs"]
fn local_sources_share_existing_mcp_tools_without_remote_attestation_or_git_jobs() {
    let mut f = LocalSnapshotFixture::new();
    let published = f.adopt();
    let project = f.work.project.id.clone();
    let repository = published.repository_id.clone();
    let actor = f
        .work
        .store
        .create_actor(
            &f.work.ctx,
            ActorKind::Agent,
            "MCP local reader",
            "Explicit project-scoped fixture",
        )
        .unwrap();
    let capabilities = vec![
        Capability::RepositoriesRead,
        Capability::RepositoriesSync,
        Capability::KnowledgeRead,
        Capability::KnowledgeWrite,
        Capability::DocumentsRead,
        Capability::DocumentsWrite,
        Capability::WorkflowsRead,
    ];
    for capability in &capabilities {
        f.work
            .store
            .grant(&f.work.ctx, &actor.id, *capability, Some(&project))
            .unwrap();
    }
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            &actor.id,
            "MCP local fixture",
            KeyScope {
                capabilities,
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let workflow = f.work.store.create_workflow(&f.work.ctx,&project,&serde_json::from_value(json!({
        "name":"Sourced MCP requirements","purpose":"Capture verified local evidence","resource_kinds":["requirement"],
        "graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],
            "transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"evidence_valid"}]}]}})).unwrap()).unwrap();
    let flow = f
        .work
        .store
        .publish_workflow(
            &f.work.ctx,
            &project,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Use an explicitly published requirement flow".into(),
            },
        )
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let http = |method: &str, path: &str, body: Option<Value>| {
        executor.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("authorization", format!("Bearer {}", key.key))
                        .header("content-type", "application/json")
                        .header("accept", "application/json, text/event-stream")
                        .header("mcp-protocol-version", "2025-11-25")
                        .body(Body::from(
                            body.map(|v| serde_json::to_vec(&v).unwrap())
                                .unwrap_or_default(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        })
    };
    let rpc = |name: &str, arguments: Value| {
        http(
            "POST",
            "/mcp",
            Some(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
            ),
        )
    };
    let (_, catalog) = http(
        "POST",
        "/mcp",
        Some(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})),
    );
    assert_eq!(catalog["result"]["tools"].as_array().unwrap().len(), 88);
    assert!(
        !catalog["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"].as_str().unwrap().contains("adopt"))
    );
    let scope = json!({"project_id":project,"repository_id":repository});
    let (_, metadata) = rpc("nexora_repository_get", scope.clone());
    assert_eq!(
        metadata["result"]["structuredContent"]["source_kind"],
        "local_snapshot"
    );
    assert_eq!(
        metadata["result"]["structuredContent"]["local_source"]["payload_hash"],
        f.header.payload_hash
    );
    let stored = f
        .runtime
        .snapshot(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap()
        .unwrap();
    let node = stored
        .graph
        .nodes
        .iter()
        .find(|n| n.name == "retained_identity")
        .unwrap()
        .id
        .0
        .clone();
    for (name, extra) in [
        ("nexora_repository_overview", json!({"limit":10})),
        ("nexora_find_symbol", json!({"name":"retained_identity"})),
        ("nexora_find_references", json!({"node":node})),
        ("nexora_get_dependencies", json!({"node":node})),
        (
            "nexora_traverse_graph",
            json!({"node":node,"depth":2,"limit":10}),
        ),
        ("nexora_search_code", json!({"query":"retained_identity"})),
        (
            "nexora_get_source_context",
            json!({"path":"source.js","start_line":1,"line_count":1}),
        ),
    ] {
        let mut args = scope.clone();
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let (status, value) = rpc(name, args);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["isError"], false, "{name}: {value}");
        let result = &value["result"]["structuredContent"];
        assert_eq!(result["snapshot"], f.header.id.0);
        assert_eq!(result["repository"], f.header.repository_id.0);
        assert!(result["commit"].is_null());
        assert_eq!(result["local_origin"]["source_id"], published.source_id);
    }
    for extra in [
        json!({"root":"/remote/input"}),
        json!({"tenant":"remote-tenant"}),
        json!({"actor":"forged-actor"}),
    ] {
        let mut args = scope.clone();
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            rpc("nexora_repository_overview", args).1["error"]["code"],
            -32602
        );
    }
    for name in ["nexora_list_pending", "nexora_list_knowledge"] {
        assert_eq!(rpc(name, scope.clone()).1["result"]["isError"], false);
    }
    let draft = json!({"kind":"decision","statement":"An external actor supplied this claim for an adopted snapshot.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]});
    let mut args = scope.clone();
    args["request_id"] = json!("local-mcp-contribute");
    args["draft"] = draft.clone();
    let (_, first) = rpc("nexora_contribute_knowledge", args.clone());
    assert_eq!(first["result"]["isError"], false);
    assert_eq!(
        rpc("nexora_contribute_knowledge", args).1["result"],
        first["result"]
    );
    let id = first["result"]["structuredContent"]["contribution"]["id"]
        .as_str()
        .unwrap();
    let mut args = scope.clone();
    args["id"] = json!(id);
    args["request_id"] = json!("local-mcp-revise");
    args["draft"] = draft;
    args["expected_revision"] = json!(1);
    assert_eq!(
        rpc("nexora_revise_knowledge", args).1["result"]["structuredContent"]["contribution"]["revision"],
        2
    );
    let mut args = scope.clone();
    args["id"] = json!(id);
    args["request_id"] = json!("local-mcp-review");
    args["expected_revision"] = json!(2);
    args["action"] = json!("accept");
    args["reason"] = json!("External review of a sourced claim");
    assert_eq!(
        rpc("nexora_review_knowledge", args).1["result"]["structuredContent"]["contribution"]["revision"],
        3
    );
    let mut args = scope.clone();
    args["id"] = json!(id);
    assert_eq!(
        rpc("nexora_get_knowledge", args).1["result"]["structuredContent"]["history_total"],
        3
    );
    let (_, manifest) = rpc(
        "nexora_context_manifest_create",
        json!({"project_id":project,"request_id":"mcp-local-context",
        "selection":{"name":"MCP local context","repository_ids":[repository],"reason":"Capture authorized local provenance"}}),
    );
    assert_eq!(manifest["result"]["isError"], false);
    assert_eq!(
        manifest["result"]["structuredContent"]["members"][0]["local_origin"]["publication_id"],
        published.publication_id
    );
    let manifest_id = manifest["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let (_, document) = rpc(
        "nexora_document_create",
        json!({"project_id":project,"request_id":"mcp-local-requirement","definition":{
            "code":"REQ-MCP-LOCAL","title":"A sourced external requirement","workflow_version_id":flow.id,"origin":"observed_code",
            "context_manifest_id":manifest_id,"evidence":[{"repository_id":repository,"path":"source.js","start_line":1,"line_count":1}],
            "content":{"kind":"requirement","body":{"problem":"Preserve imported source provenance","objective":"Record exact evidence",
                "scope":"Public fixture function","business_actors":["Developer"],"acceptance_criteria":["Evidence names its attested source"],
                "facts":["The persisted fixture declares a function"],"source":"External author; evidence existence is not proof of requirement completion"}},
            "reason":"Record a structured claim through authenticated MCP"
        }}),
    );
    assert_eq!(document["result"]["isError"], false, "{document}");
    let definition = &document["result"]["structuredContent"];
    let version_arguments = json!({"project_id":project,"document_id":definition["id"],"version_id":definition["latest_version_id"]});
    let (_, version) = rpc("nexora_document_version_get", version_arguments.clone());
    assert_eq!(
        version["result"]["structuredContent"]["created_by"],
        actor.id
    );
    assert_eq!(
        version["result"]["structuredContent"]["evidence"][0]["local_origin"]["source_id"],
        published.source_id
    );
    for name in ["nexora_sync_repository", "nexora_index_repository"] {
        let mut args = scope.clone();
        args["request_id"] = json!(format!("not-git-{name}"));
        args["request"] = json!({"expected_revision":1});
        assert_eq!(rpc(name, args).1["result"]["isError"], true);
    }
    let count: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM repository_jobs WHERE repository_id=$1",
            &[&repository],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    assert_eq!(
        http(
            "POST",
            &format!("/api/v1/projects/{project}/repositories/{repository}/adopt-local"),
            Some(json!({"root":"/remote/input"}))
        )
        .0,
        StatusCode::NOT_FOUND
    );
    let change = LocalSourceChange {
        expected_revision: published.source_revision,
        reason: "Retire source after protocol acceptance".into(),
    };
    f.work
        .store
        .request(
            &f.work.ctx.clone(),
            "local_source.revoke",
            "revoke-mcp-local",
            &json!({"project_id":project,"repository_id":repository,"change":change}),
            |store| store.revoke_local_source(&f.work.ctx, &project, &repository, &change),
        )
        .unwrap();
    assert_eq!(
        rpc("nexora_repository_overview", scope).1["result"]["isError"],
        true
    );
    assert_eq!(
        rpc("nexora_document_version_get", version_arguments).1["result"]["structuredContent"]["needs_review"],
        true
    );
    std::fs::remove_dir_all(&f.root).unwrap();
}
