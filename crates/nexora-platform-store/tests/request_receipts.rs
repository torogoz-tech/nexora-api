use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires local PostgreSQL; isolated installation and generated credentials"]
fn repeated_project_request_returns_the_original_event_without_duplicate_mutation() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_receipt_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Receipt fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let body = serde_json::json!({"name":"Once","description":null});
    let project = store
        .request(&ctx, "project.create", "request-1", &body, |store| {
            store.create_project(&ctx, "Once", None)
        })
        .unwrap();
    let replay = store
        .request(&ctx, "project.create", "request-1", &body, |store| {
            store.create_project(&ctx, "Once", None)
        })
        .unwrap();
    assert_eq!(project.id, replay.id);
    assert_eq!(store.list_projects(&ctx).unwrap().len(), 1);
    assert!(
        store
            .request(
                &ctx,
                "project.create",
                "request-1",
                &serde_json::json!({"name":"Other"}),
                |store| store.create_project(&ctx, "Other", None)
            )
            .is_err()
    );
    assert_eq!(store.list_projects(&ctx).unwrap().len(), 1);
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    let replay = store
        .request(&ctx, "project.create", "request-1", &body, |store| {
            store.create_project(&ctx, "Once", None)
        })
        .unwrap();
    assert_eq!(project.id, replay.id);
}

#[test]
#[ignore = "requires local PostgreSQL; retries must not duplicate identities or re-deliver secrets"]
fn identity_requests_replay_but_key_material_is_issued_only_once() {
    use nexora_platform::{ActorKind, Capability, KeyScope};
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_once_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Once fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let actor_body =
        serde_json::json!({"kind":"agent","name":"One agent","purpose":"Retry fixture"});
    let actor = store
        .request(&ctx, "actor.create", "actor-1", &actor_body, |store| {
            store.create_actor(&ctx, ActorKind::Agent, "One agent", "Retry fixture")
        })
        .unwrap();
    let again = store
        .request(&ctx, "actor.create", "actor-1", &actor_body, |store| {
            store.create_actor(&ctx, ActorKind::Agent, "One agent", "Retry fixture")
        })
        .unwrap();
    assert_eq!(actor.id, again.id);
    let user_body = serde_json::json!({"username":"person","password":password,"name":"Person"});
    let person = store
        .request(&ctx, "user.create", "person-1", &user_body, |store| {
            store.create_user(&ctx, "person", &password, "Person")
        })
        .unwrap();
    let person_again = store
        .request(&ctx, "user.create", "person-1", &user_body, |store| {
            store.create_user(&ctx, "person", &password, "Person")
        })
        .unwrap();
    assert_eq!(person.id, person_again.id);
    let scope = KeyScope {
        capabilities: vec![Capability::ProjectsRead],
        project_ids: vec![],
        all_projects: true,
        installation: false,
    };
    let key_body = serde_json::json!({"actor_id":ctx.actor_id(),"name":"One credential","scope":scope,"expires_in_seconds":3600});
    let issued = store
        .request(&ctx, "key.issue", "key-1", &key_body, |store| {
            store.issue_key(&ctx, ctx.actor_id(), "One credential", scope.clone(), 3600)
        })
        .unwrap();
    assert!(
        store
            .request(&ctx, "key.issue", "key-1", &key_body, |store| store
                .issue_key(&ctx, ctx.actor_id(), "One credential", scope, 3600))
            .is_err()
    );
    assert_eq!(store.list_keys(&ctx, ctx.actor_id()).unwrap().len(), 1);
    assert!(store.authenticate(&issued.key).is_ok());
}
