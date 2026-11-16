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
#[ignore = "requires local databases; MCP code tools use published logical scope and reject revoked or foreign access even with warm cache"]
fn installation_code_mcp_queries_persisted_code_with_live_scoped_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_code_mcp_{nonce}");
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
    std::fs::write(root.join("source.js"), "function changed() {}\n").unwrap();
    let prepared = runtime
        .stage_repository(&indexer, &root, tenant.clone(), runtime_repository.clone())
        .unwrap();
    let prepared_header = runtime
        .query_header(&tenant, &runtime_repository, Some(&prepared.snapshot.id))
        .unwrap();
    assert_ne!(prepared_header.id, header.id);
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Code MCP fixture",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    // Queries must use persisted projections, even after the source directory is gone.
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
    let rpc = |token: &str, message: Value| {
        executor.block_on(async {
            let request = Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", format!("Bearer {token}"))
                .header("accept", "application/json, text/event-stream")
                .header("content-type", "application/json")
                .header("mcp-protocol-version", "2025-11-25")
                .body(Body::from(serde_json::to_vec(&message).unwrap()))
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            let status = response.status();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&body).unwrap())
        })
    };
    let (_, catalog) = rpc(
        &key.key,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    );
    let tools = catalog["result"]["tools"].as_array().unwrap();
    for name in [
        "nexora_repository_overview",
        "nexora_find_symbol",
        "nexora_find_references",
        "nexora_get_dependencies",
        "nexora_traverse_graph",
        "nexora_search_code",
        "nexora_get_source_context",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing registered code tool: {name}"
        );
    }
    let call = |name: &str, arguments: Value| {
        rpc(
            &key.key,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":name,"arguments":arguments}}),
        )
    };
    let scope = json!({"project_id":project.id,"repository_id":repository.id});
    let (status, overview) = call("nexora_repository_overview", scope.clone());
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["result"]["isError"], false);
    assert_eq!(
        overview["result"]["structuredContent"]["snapshot"],
        header.id.0
    );
    let mut find = scope.clone();
    find["name"] = json!("published");
    let (_, symbols) = call("nexora_find_symbol", find);
    assert_eq!(symbols["result"]["isError"], false);
    let items = symbols["result"]["structuredContent"]["items"]
        .as_array()
        .unwrap();
    assert!(!items.is_empty());
    let node = items[0]["id"].as_str().unwrap();
    for name in [
        "nexora_find_references",
        "nexora_get_dependencies",
        "nexora_traverse_graph",
    ] {
        let mut args = scope.clone();
        args["node"] = json!(node);
        let (_, response) = call(name, args);
        assert_eq!(
            response["result"]["isError"], false,
            "graph query failed: {name}"
        );
        assert_eq!(
            response["result"]["structuredContent"]["snapshot"],
            header.id.0
        );
    }
    let mut search = scope.clone();
    search["query"] = json!("function published");
    let (_, found) = call("nexora_search_code", search);
    assert_eq!(found["result"]["isError"], false);
    assert!(
        !found["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut context = scope.clone();
    context["path"] = json!("source.js");
    let (_, source) = call("nexora_get_source_context", context);
    assert_eq!(source["result"]["isError"], false);
    assert!(
        source["result"]["structuredContent"]
            .to_string()
            .contains("function published")
    );
    let mut unpublished = scope.clone();
    unpublished["snapshot"] = json!(prepared_header.id.0);
    let (_, denied) = call("nexora_repository_overview", unpublished);
    assert_eq!(
        denied["result"]["isError"], true,
        "runtime preparation is not platform publication"
    );
    let next_job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let next_lease = store
        .claim_repository_job("next-publication", 300)
        .unwrap()
        .unwrap();
    assert_eq!(next_lease.job.id, next_job.id);
    let next_commit = "b".repeat(40);
    store.pin_repository_job(&next_lease, &next_commit).unwrap();
    store
        .complete_repository_job(&next_lease, &prepared_header)
        .unwrap();
    let mut historical = scope.clone();
    historical["snapshot"] = json!(header.id.0);
    historical["path"] = json!("source.js");
    let (_, previous) = call("nexora_get_source_context", historical);
    assert_eq!(previous["result"]["isError"], false);
    assert_eq!(previous["result"]["structuredContent"]["commit"], commit);
    assert!(
        previous["result"]["structuredContent"]
            .to_string()
            .contains("function published")
    );
    let (_, current) = call("nexora_repository_overview", scope.clone());
    assert_eq!(
        current["result"]["structuredContent"]["snapshot"],
        prepared_header.id.0
    );
    assert_eq!(
        current["result"]["structuredContent"]["commit"],
        next_commit
    );
    let mut injected = scope.clone();
    injected["tenant"] = json!("foreign");
    let (_, denied) = call("nexora_repository_overview", injected);
    assert_eq!(denied["error"]["code"], -32602);
    let mut foreign = scope.clone();
    foreign["project_id"] = json!("foreign-project");
    let (_, denied) = call("nexora_repository_overview", foreign);
    assert_eq!(denied["result"]["isError"], true);
    sql.execute("UPDATE project_repositories SET authorization_status='revoked',source_generation=source_generation+1,revision=revision+1 WHERE id=$1", &[&repository.id]).unwrap();
    let (_, denied) = call("nexora_repository_overview", scope.clone());
    assert_eq!(
        denied["result"]["isError"], true,
        "warm result cache must not bypass revoked source authority"
    );
    store.revoke_key(&ctx, &key.metadata.id).unwrap();
    assert_eq!(
        call("nexora_repository_overview", scope).0,
        StatusCode::UNAUTHORIZED
    );
}
