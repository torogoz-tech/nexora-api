use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::path::Path;

#[test]
#[ignore = "requires isolated local PostgreSQL"]
fn knowledge_can_be_created_revised_and_published_without_a_workflow() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_docs").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = SecretGenerator::token("fixture_password").unwrap();
    store
        .bootstrap("Knowledge fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let session = store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let ctx = store
        .authenticate(session["access_token"].as_str().unwrap())
        .unwrap();
    let project = store
        .create_project(&ctx, "Knowledge without workflows", None)
        .unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Knowledge MCP",
            KeyScope {
                capabilities: vec![
                    Capability::DocumentsRead,
                    Capability::DocumentsWrite,
                    Capability::DocumentsPublish,
                ],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let ctx = store.authenticate(&key.key).unwrap();
    let content = json!({"kind":"requirement","body":{"problem":"Missing context","objective":"Share knowledge","scope":"Team","business_actors":["Developer"],"acceptance_criteria":["Knowledge is versioned"],"source":"Business owner"}});
    let input: ProjectDocumentCreate = serde_json::from_value(json!({"code":"req-knowledge","title":"Shared knowledge","content":content,"origin":"declared_business","reason":"Initial definition"})).expect("knowledge must not require workflow_version_id");
    let document = store
        .create_project_document(&ctx, &project.id, &input)
        .unwrap();
    let first = store
        .document_version(&ctx, &project.id, &document.id, &document.latest_version_id)
        .unwrap();
    assert!(first.workflow_version_id.is_none());
    let publication = store
        .publish_project_document(
            &ctx,
            &project.id,
            &document.id,
            &serde_json::from_value::<ProjectDocumentPublish>(json!({
                "expected_revision": document.revision,
                "version_id": first.id,
                "reason": "Publish knowledge"
            }))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(publication.version_id, first.id);
    let current = store
        .get_project_document(&ctx, &project.id, &document.id)
        .unwrap();
    let revision: ProjectDocumentRevise = serde_json::from_value(json!({"expected_revision":current.revision,"title":"Updated shared knowledge","content":content,"origin":"declared_business","reason":"Clarify knowledge"})).unwrap();
    let second = store
        .revise_project_document(&ctx, &project.id, &document.id, &revision)
        .unwrap();
    assert!(second.workflow_version_id.is_none());
    assert_eq!(second.number, 2);
    assert_eq!(
        second.previous_version_id.as_deref(),
        Some(first.id.as_str())
    );
    let historical = store
        .document_version(&ctx, &project.id, &document.id, &first.id)
        .unwrap();
    assert_eq!(historical.checksum, first.checksum);
    assert_eq!(historical.title, "Shared knowledge");
    let stale = store.revise_project_document(&ctx, &project.id, &document.id, &revision);
    assert!(matches!(stale, Err(PlatformError::Conflict(_))));
}
