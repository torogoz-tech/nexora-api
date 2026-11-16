#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "../../nexora-platform-store/tests/support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use gitlab_source_fixture::GitlabSourceFixture;
use nexora_api::RepositoryWorker;
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::{
    GitProvider, GitlabClientRegistration, RepositoryJobKind, RepositoryJobRequest,
    RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator,
};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use serde_json::json;
use std::{
    os::unix::fs::DirBuilderExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; provider proof through encrypted credential, Git fetch, exact source and scoped publication"]
fn worker_publishes_one_mixed_snapshot_with_frozen_candidates_and_safe_mcp_provenance() {
    let git = GitFixture::new("job-source-plan");
    let code = b"export function generated() { return 7; }\n";
    let blob = git.blob(code);
    let lua = git.blob(b"function mixed() return 1 end\n");
    let unknown = git.blob(b"uncovered\n");
    let attributes = git.blob(b"source.js export-ignore\n");
    let tree = git.tree(&[
        ("100644", &attributes, ".gitattributes"),
        ("100644", &blob, "source.js"),
        ("100644", &lua, "source.lua"),
        ("100644", &unknown, "notes.unknown"),
    ]);
    let commit = git.commit(&tree);
    git.run(&["update-ref", "refs/heads/main", &commit], b"");
    git.run(&["symbolic-ref", "HEAD", "refs/heads/main"], b"");
    let provider = GitlabSourceFixture::start(&git, &commit);
    provider.control(json!({}));
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_worker_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Source plan fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let policy = GitNetworkPolicy {
        gitlab_origin: provider.origin().into(),
        ..GitNetworkPolicy::default()
    };
    let client_secret = Zeroizing::new(SecretGenerator::token("fixture_client_secret").unwrap());
    store
        .configure_gitlab(
            &ctx,
            &GitlabClientRegistration {
                client_id: "fixture-client".into(),
                client_secret: client_secret.to_string(),
                redirect_uri: format!("{}/api/v1/git/gitlab/callback", policy.callback_origin),
                enabled: true,
                expected_revision: 0,
            },
            &policy,
        )
        .unwrap();
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
    let connection = SecretGenerator::id("conn").unwrap();
    let host = policy.host().unwrap();
    let tokens = Zeroizing::new(
        serde_json::to_vec(&json!({
            "access_token":provider.token(),"refresh_token":null,"token_type":"Bearer",
            "scope":"read_api read_repository","expires_in":3600
        }))
        .unwrap(),
    );
    let encrypted = serde_json::to_value(
        store
            .provider_secret_cipher()
            .unwrap()
            .seal(
                &ProviderSecretContext {
                    binding_id: connection.clone(),
                    provider: GitProvider::Gitlab.into(),
                    host: host.clone(),
                    purpose: "gitlab_tokens".into(),
                    revision: 1,
                },
                &tokens,
            )
            .unwrap(),
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
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab',$2,$3,'42','fixture','active',1,$4,now()+interval '1 hour')", &[&connection,&host,&ctx.actor_id(),&encrypted]).unwrap();
    sql.execute("UPDATE project_repositories SET host=$3,remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id,&host]).unwrap();
    let mut imported = Vec::new();
    for language in ["javascript", "lua"] {
        let package = nexora_language::AnalysisProfilePackage::from_catalog(
            &Path::new("../../languages").join(language),
            language.into(),
            format!("mixed-{language}"),
            "1.9.0".into(),
            format!("Mixed {language}"),
        )
        .unwrap();
        imported.push(store.import_analysis_profile(&ctx, &package).unwrap());
    }
    let input = RepositoryJobRequest {
        expected_revision: 2,
    };
    let assignment = store
        .repository_analysis_profile(&ctx, &project.id, &repository.id)
        .unwrap();
    assert_eq!(assignment["mode"], "automatic");
    let job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    // Importing a newer model cannot change a queued job's candidate set.
    let newer = nexora_language::AnalysisProfilePackage::from_catalog(
        Path::new("../../languages/javascript"),
        "javascript".into(),
        "mixed-javascript".into(),
        "1.10.0".into(),
        "New JavaScript".into(),
    )
    .unwrap();
    store.import_analysis_profile(&ctx, &newer).unwrap();
    let managed = git.directory.path().canonicalize().unwrap().join("managed");
    let mut directory = std::fs::DirBuilder::new();
    directory.mode(0o700).create(&managed).unwrap();
    let settings = serde_json::from_value(json!({
        "id":"mixed-profile-worker", "source_root":managed,
        "git_binary":"/usr/bin/git", "profiles":{}
    }))
    .unwrap();
    let mut worker = RepositoryWorker::connect(&config, &namespace, settings, policy).unwrap();
    let result = worker.run_once().unwrap().unwrap();
    assert_eq!(result.id, job.id);
    assert_eq!(
        result.state,
        RepositoryJobState::Succeeded,
        "{:?}",
        result.error_code
    );
    let row = sql.query_one("SELECT runtime_tenant_id,runtime_repository_id,snapshot_id FROM repository_bindings WHERE repository_id=$1", &[&repository.id]).unwrap();
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    let snapshot = runtime
        .snapshot(
            &TenantId(row.get(0)),
            &RepositoryId(row.get(1)),
            Some(&SnapshotId(row.get(2))),
        )
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.sources.len(), 2);
    assert!(snapshot.sources.contains_key("source.js"));
    assert!(snapshot.sources.contains_key("source.lua"));
    let assignment = store
        .repository_analysis_profile(&ctx, &project.id, &repository.id)
        .unwrap();
    let report = &assignment["last_selection"];
    assert_eq!(report["policy"], "automatic-latest/v1");
    assert_eq!(report["profiles"].as_array().unwrap().len(), 2);
    assert!(
        report["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["version"] == "1.9.0")
    );
    assert!(
        report["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("package").is_none())
    );
    assert!(
        report["uncovered_files"]
            .as_array()
            .unwrap()
            .contains(&json!("notes.unknown"))
    );
    assert_eq!(std::fs::read_dir(&managed).unwrap().count(), 0);
    // The public HTTP/MCP query returns the same safe metadata, never installed models.
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Safe provenance",
            nexora_platform::KeyScope {
                capabilities: vec![
                    nexora_platform::Capability::RepositoriesRead,
                    nexora_platform::Capability::KnowledgeRead,
                ],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        use axum::{body::Body, http::Request};
        use http_body_util::BodyExt;
        use nexora_api::{ApiState,ApiServer};
        use tower::ServiceExt;
        let app = ApiServer::router(ApiState::new(config.clone(), namespace.clone(),4).await.unwrap());
        let path = format!("/api/v1/projects/{}/repositories/{}/analysis-profile", project.id, repository.id);
        let response = app.clone().oneshot(Request::get(path).header("authorization",format!("Bearer {}",session.access_token)).body(Body::empty()).unwrap()).await.unwrap();
        let status=response.status();
        let bytes=response.into_body().collect().await.unwrap().to_bytes();
        assert!(status.is_success(), "{status}: {}",String::from_utf8_lossy(&bytes));
        let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["last_selection"],*report);
        let response=app.clone().oneshot(Request::post("/mcp")
            .header("authorization",format!("Bearer {}",key.key))
            .header("content-type","application/json")
            .header("accept","application/json, text/event-stream")
            .header("MCP-Protocol-Version","2025-11-25")
            .body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_repository_analysis_profile_get","arguments":{"project_id":project.id,"repository_id":repository.id}}}).to_string())).unwrap()).await.unwrap();
        let status=response.status();
        let bytes=response.into_body().collect().await.unwrap().to_bytes();
        assert!(status.is_success());
        let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["result"]["structuredContent"]["last_selection"],*report,"{value}");
        assert!(!String::from_utf8_lossy(&bytes).contains("\"models\""));
        let change=json!({"expected_revision":assignment["revision"],"mode":"manual","profile_version_ids":imported.iter().map(|v|v["id"].clone()).collect::<Vec<_>>(),"reason":"Freeze selected models"});
        let request=||Request::put(format!("/api/v1/projects/{}/repositories/{}/analysis-profile",project.id,repository.id))
            .header("authorization",format!("Bearer {}",session.access_token))
            .header("content-type","application/json").header("idempotency-key","mixed-manual-selection")
            .body(Body::from(change.to_string())).unwrap();
        let response=app.clone().oneshot(request()).await.unwrap();
        let status=response.status();
        let bytes=response.into_body().collect().await.unwrap().to_bytes();
        assert!(status.is_success(),"{status}: {}",String::from_utf8_lossy(&bytes));
        let changed:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        assert_eq!(changed["mode"],"manual");
        assert_eq!(changed["profiles"].as_array().unwrap().len(),2);
        let replay=app.oneshot(request()).await.unwrap();
        assert!(replay.status().is_success());
        let bytes=replay.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),changed);
    });
}
