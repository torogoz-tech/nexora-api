use nexora_platform::{Capability, KeyScope};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires PostgreSQL; upgrade isolated schema2 fixture without resetting identities or receipts"]
fn registry_migration_preserves_existing_identity_credentials_and_idempotency_receipts() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_upgrade_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = SecretGenerator::token("test_password").unwrap();
    store
        .bootstrap("Upgrade fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let body = serde_json::json!({"name":"Existing project"});
    let project = store
        .request(&ctx, "project.create", "before-upgrade", &body, |s| {
            s.create_project(&ctx, "Existing project", None)
        })
        .unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Existing key",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    // Only this generated fixture is reduced to the frozen v2 schema. No installed namespace is altered.
    let password =
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap();
    let mut client = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client.batch_execute(&format!("DROP FUNCTION \"{namespace}\".repository_metadata(\"{namespace}\".project_repositories); ALTER TABLE \"{namespace}\".local_repository_sources DROP CONSTRAINT local_current_publication; DROP TABLE \"{namespace}\".local_snapshot_publications, \"{namespace}\".local_repository_sources; DROP FUNCTION \"{namespace}\".retain_local_source_identity(); DROP TABLE \"{namespace}\".approval_requests, \"{namespace}\".document_approvals; DROP FUNCTION \"{namespace}\".retain_approval_request_operation(); DROP TABLE \"{namespace}\".project_context_packages, \"{namespace}\".document_links; DROP FUNCTION \"{namespace}\".retain_document_link_identity(); DROP TABLE \"{namespace}\".document_history, \"{namespace}\".document_publications, \"{namespace}\".document_version_states; ALTER TABLE \"{namespace}\".project_documents DROP CONSTRAINT document_latest_version_fk, DROP CONSTRAINT document_published_version_fk; DROP TABLE \"{namespace}\".project_document_versions; DROP FUNCTION \"{namespace}\".project_context_status(text,text); DROP TABLE \"{namespace}\".project_context_manifests; DROP TABLE \"{namespace}\".project_documents; DROP FUNCTION \"{namespace}\".forbid_document_record_mutation(), \"{namespace}\".validate_document_version_state(); DROP TABLE \"{namespace}\".task_comments; DROP FUNCTION \"{namespace}\".forbid_task_comment_mutation(); DROP TABLE \"{namespace}\".task_approvals; DROP TABLE \"{namespace}\".task_dependencies, \"{namespace}\".task_history, \"{namespace}\".tasks; DROP FUNCTION \"{namespace}\".validate_task_workflow_state(); DROP FUNCTION \"{namespace}\".forbid_task_template_snapshot_mutation(); ALTER TABLE \"{namespace}\".task_templates DROP CONSTRAINT task_template_latest_version_fk; DROP TABLE \"{namespace}\".task_template_versions; DROP TABLE \"{namespace}\".task_templates; DROP FUNCTION \"{namespace}\".forbid_task_template_version_mutation(); DROP TABLE \"{namespace}\".backlogs, \"{namespace}\".task_types; ALTER TABLE \"{namespace}\".workflows DROP CONSTRAINT workflow_latest_version_fk; DROP TABLE \"{namespace}\".workflow_versions; DROP TABLE \"{namespace}\".workflows; DROP FUNCTION \"{namespace}\".forbid_workflow_version_mutation(); DROP TABLE \"{namespace}\".repository_bindings; DROP TABLE \"{namespace}\".repository_jobs; DROP TABLE \"{namespace}\".git_webhook_deliveries, \"{namespace}\".gitlab_webhooks, \"{namespace}\".repository_source_events; DROP TABLE \"{namespace}\".github_manifest_authorizations, \"{namespace}\".github_consents, \"{namespace}\".github_settings; ALTER TABLE \"{namespace}\".sessions DROP COLUMN authentication_provider_id; DROP TABLE \"{namespace}\".oidc_authorizations, \"{namespace}\".oidc_identities; DROP TABLE \"{namespace}\".oidc_providers, \"{namespace}\".authentication_policy; DROP TABLE \"{namespace}\".project_repositories; DROP FUNCTION \"{namespace}\".retain_repository_source_kind(); DROP TABLE \"{namespace}\".git_connection_projects, \"{namespace}\".git_authorizations; DROP TABLE \"{namespace}\".git_connections, \"{namespace}\".gitlab_settings; DELETE FROM \"{namespace}\".platform_schema_versions WHERE version>=3")).unwrap();
    assert!(!store.ready().unwrap());
    store.initialize().unwrap();
    assert!(store.ready().unwrap());
    assert_eq!(
        store.authenticate(&key.key).unwrap().actor_id(),
        ctx.actor_id()
    );
    let replay = store
        .request(&ctx, "project.create", "before-upgrade", &body, |s| {
            s.create_project(&ctx, "Existing project", None)
        })
        .unwrap();
    assert_eq!(replay.id, project.id);
    assert_eq!(store.get_project(&ctx, &project.id).unwrap().revision, 1);
    store.initialize().unwrap();
    assert!(store.ready().unwrap());
    assert!(
        store
            .repository_page(&ctx, &project.id, None, 100, false)
            .unwrap()
            .items
            .is_empty()
    );
}
