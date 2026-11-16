use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    Capability, GitProvider, KeyScope, PlatformError, RepositoryJobKind, RepositoryJobRequest,
    RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; explicit source context and verified evidence use common HTTP/MCP services"]
fn source_context_and_evidence_are_shared_by_http_and_mcp() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_context_http_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Publication fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Source", None).unwrap();
    let repository = store
        .register_repository(
            &ctx,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let pg_password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')", &[&connection,&ctx.actor_id()]).unwrap();
    sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id]).unwrap();
    let input = RepositoryJobRequest {
        expected_revision: 2,
    };
    let job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    let first = store
        .claim_repository_job("publisher", 300)
        .unwrap()
        .unwrap();
    assert_eq!(first.job.id, job.id);
    assert!(matches!(
        store.pin_repository_job(&first, "invalid"),
        Err(PlatformError::Validation(_))
    ));
    let commit = "a".repeat(40);
    let scope = store.pin_repository_job(&first, &commit).unwrap();
    let installation: String = sql
        .query_one("SELECT id FROM installation WHERE singleton", &[])
        .unwrap()
        .get(0);
    assert_eq!(scope.tenant_id, installation);
    assert_eq!(scope.repository_id, repository.id);
    assert_eq!(scope.pinned_commit, commit);
    assert!(matches!(
        store.pin_repository_job(&first, &"b".repeat(40)),
        Err(PlatformError::Conflict(_))
    ));
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-publication-{nonce}"));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("source.js"), "function published() {}\n").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    let tenant = TenantId(scope.tenant_id.clone());
    let runtime_repository = RepositoryId(scope.repository_id.clone());
    let staged = runtime
        .stage_repository(&indexer, &root, tenant.clone(), runtime_repository.clone())
        .unwrap();
    let header = runtime
        .query_header(&tenant, &runtime_repository, Some(&staged.snapshot.id))
        .unwrap();

    store.complete_repository_job(&first, &header).unwrap();

    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Source-backed document","purpose":"Verified references","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"evidence_valid"},{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow=store.publish_workflow(&ctx,&project.id,&workflow.id,&serde_json::from_value(json!({"expected_revision":workflow.revision,"reason":"Verify references before review"})).unwrap()).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Source context API fixture",
            KeyScope {
                capabilities: vec![
                    Capability::DocumentsRead,
                    Capability::DocumentsWrite,
                    Capability::DocumentsPublish,
                    Capability::RepositoriesRead,
                    Capability::KnowledgeRead,
                ],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    // The provider registration is an isolated local fixture; source verification reads persisted bytes.
    std::fs::remove_dir_all(&root).unwrap();
    drop(runtime);
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(config.clone(), namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request = |method: &str, path: &str, token: &str, body: Option<Value>, id: Option<&str>| {
        executor.block_on(async {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25");
            if let Some(id) = id {
                builder = builder.header("idempotency-key", id);
            }
            let response = app
                .clone()
                .oneshot(
                    builder
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

    let base = format!("/api/v1/projects/{}/context-manifests", project.id);
    let selection = json!({"name":"Pinned source context","repository_ids":[repository.id],"reason":"External actor selects explicit code context"});
    let (status, manifest) = request(
        "POST",
        &base,
        &key.key,
        Some(selection.clone()),
        Some("context-once"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let id = manifest["id"].as_str().unwrap();
    assert_eq!(manifest["members"][0]["snapshot_id"], header.id.0);
    let rpc = |name: &str, args: Value| {
        request(
            "POST",
            "/mcp",
            &key.key,
            Some(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
            ),
            None,
        )
    };
    let (_, replay) = rpc(
        "nexora_context_manifest_create",
        json!({"project_id":project.id,"request_id":"context-once","selection":selection}),
    );
    assert_eq!(replay["result"]["isError"], false);
    assert_eq!(replay["result"]["structuredContent"], manifest);
    let (status, read) = request("GET", &format!("{base}/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read, manifest);
    let (_, read) = rpc(
        "nexora_context_manifest_get",
        json!({"project_id":project.id,"manifest_id":id}),
    );
    assert_eq!(read["result"]["structuredContent"], manifest);
    let (status, current) = request("GET", &format!("{base}/{id}/status"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["needs_review"], false);
    let (_, current) = rpc(
        "nexora_context_manifest_status",
        json!({"project_id":project.id,"manifest_id":id}),
    );
    assert_eq!(
        current["result"]["structuredContent"]["needs_review"],
        false
    );
    let definition = json!({"code":"REQ-SOURCE","title":"Source-backed requirement","content":{"kind":"requirement","body":{"problem":"Source behavior needs documentation","objective":"Record the selected code context","scope":"Fixture function","business_actors":["Developer"],"acceptance_criteria":["Reference remains pinned"],"source":"External actor observation"}},"origin":"observed_code","workflow_version_id":flow.id,"context_manifest_id":id,"evidence":[{"repository_id":repository.id,"path":"source.js","start_line":1,"line_count":1}],"reason":"Reference persisted source"});
    let (_, created) = rpc(
        "nexora_document_create",
        json!({"project_id":project.id,"request_id":"source-document","definition":definition}),
    );
    assert_eq!(created["result"]["isError"], false);
    let doc = created["result"]["structuredContent"].clone();
    let doc_id = doc["id"].as_str().unwrap();
    let version = doc["latest_version_id"].as_str().unwrap();
    let doc_base = format!(
        "/api/v1/projects/{}/documents/{doc_id}/versions/{version}",
        project.id
    );
    let (status, source) = request("GET", &doc_base, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(source["needs_review"], false);
    assert_eq!(source["context_manifest_id"], id);
    assert_eq!(source["evidence"][0]["snapshot_id"], header.id.0);
    assert_eq!(
        source["evidence"][0]["context_checksum"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let (status, export) = request(
        "GET",
        &format!("{doc_base}/export?format=json"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    sql.execute("UPDATE project_repositories SET authorization_status='revoked',source_generation=source_generation+1,revision=revision+1 WHERE id=$1",&[&repository.id]).unwrap();
    let (_, stale) = rpc(
        "nexora_context_manifest_status",
        json!({"project_id":project.id,"manifest_id":id}),
    );
    assert_eq!(stale["result"]["structuredContent"]["needs_review"], true);
    let (status, stale) = request("GET", &doc_base, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stale["needs_review"], true);
    assert_eq!(stale["checksum"], source["checksum"]);
    let (status, export_again) = request(
        "GET",
        &format!("{doc_base}/export?format=json"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export_again["content"], export["content"]);
    let (status, _) = request(
        "GET",
        &format!("/api/v1/projects/foreign-project/context-manifests/{id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    store.revoke_key(&ctx, &key.metadata.id).unwrap();
    let (status, _) = request("GET", &format!("{base}/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
