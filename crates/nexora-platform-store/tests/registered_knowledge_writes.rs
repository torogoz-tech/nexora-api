#[path = "support/published_repository_fixture.rs"]
mod published_repository_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use nexora_runtime::KnowledgeCommand;
use published_repository_fixture::PublishedRepositoryFixture;
use serde_json::{Value, json};
#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; authenticated knowledge lifecycle, actor request isolation and frozen replies"]
fn registered_knowledge_writes_keep_actor_scope_versions_and_original_reply_snapshot() {
    let mut f = PublishedRepositoryFixture::new();
    let a = f.contributor("Contributor A", true);
    let b = f.contributor("Contributor B", true);
    let readonly = f.contributor("Read only", false);
    let project = f.work.project.id.clone();
    let repository = f.binding.repository_id.clone();
    let mutate = |store: &mut PlatformStore,
                  ctx: &AuthContext,
                  command: &KnowledgeCommand|
     -> Result<Value, PlatformError> {
        let operation = match command {
            KnowledgeCommand::Contribute { .. } => "knowledge.contribute",
            KnowledgeCommand::Revise { .. } => "knowledge.revise",
            KnowledgeCommand::Review { .. } => "knowledge.review",
            _ => panic!("mutation command required"),
        };
        let canonical = json!({"project_id":project,"repository_id":repository,"snapshot":Value::Null,"command":command});
        store.request(
            ctx,
            operation,
            command.request_id().unwrap(),
            &canonical,
            |store| store.query_repository_knowledge(ctx, &project, &repository, None, command),
        )
    };
    let draft = json!({"kind":"decision","statement":"Verified context remains external knowledge, not parser proof.\u{0000}","evidence":[{"path":"source.js","start_line":1,"line_count":1}]});
    let create: KnowledgeCommand = serde_json::from_value(
        json!({"operation":"contribute","request_id":"same-client-id","draft":draft}),
    )
    .unwrap();
    let first = mutate(&mut f.work.store, &a, &create).unwrap();
    let second = mutate(&mut f.work.store, &b, &create).unwrap();
    assert_eq!(first["contribution"]["actor"], a.actor_id());
    assert_eq!(second["contribution"]["actor"], b.actor_id());
    assert_ne!(first["contribution"]["id"], second["contribution"]["id"]);
    assert_eq!(mutate(&mut f.work.store, &a, &create).unwrap(), first);
    let mut altered = serde_json::to_value(&create).unwrap();
    altered["draft"]["statement"] =
        json!("Different input cannot reuse an acknowledged request ID");
    assert!(matches!(
        mutate(
            &mut f.work.store,
            &a,
            &serde_json::from_value(altered).unwrap()
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert!(matches!(
        mutate(&mut f.work.store, &readonly, &create),
        Err(PlatformError::Forbidden)
    ));
    let id = first["contribution"]["id"].as_str().unwrap();
    let original = f.binding.clone();
    let next = f.publish_revision(2);
    assert_ne!(original.snapshot_id, next.snapshot_id);
    assert_eq!(mutate(&mut f.work.store, &a, &create).unwrap(), first);
    let get = KnowledgeCommand::Get {
        id: id.into(),
        offset: 0,
        limit: 10,
    };
    let stale = f
        .work
        .store
        .query_repository_knowledge(&a, &project, &repository, None, &get)
        .unwrap();
    assert_eq!(stale["contribution"]["validity"], "needs_review");
    let revise:KnowledgeCommand=serde_json::from_value(json!({"operation":"revise","id":id,"request_id":"revise-current","expected_revision":1,"draft":{"kind":"decision","statement":"Revalidated against the newly published snapshot.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]}})).unwrap();
    let revised = mutate(&mut f.work.store, &a, &revise).unwrap();
    assert_eq!(revised["contribution"]["revision"], 2);
    assert_eq!(revised["contribution"]["snapshot"], next.snapshot_id);
    assert_eq!(revised["contribution"]["actor"], a.actor_id());
    let accept:KnowledgeCommand=serde_json::from_value(json!({"operation":"review","id":id,"request_id":"accept-current","expected_revision":2,"action":"accept","reason":"External reviewer checked the evidence"})).unwrap();
    let accepted = mutate(&mut f.work.store, &b, &accept).unwrap();
    assert_eq!(accepted["contribution"]["state"], "accepted");
    assert_eq!(accepted["contribution"]["actor"], b.actor_id());
    assert_eq!(accepted["contribution"]["effective"], true);
    assert!(mutate(&mut f.work.store, &a, &accept).is_err());
    let reject:KnowledgeCommand=serde_json::from_value(json!({"operation":"review","id":id,"request_id":"reject-current","expected_revision":3,"action":"reject","reason":"New business information rejects this interpretation"})).unwrap();
    assert_eq!(
        mutate(&mut f.work.store, &a, &reject).unwrap()["contribution"]["state"],
        "rejected"
    );
    let withdraw:KnowledgeCommand=serde_json::from_value(json!({"operation":"review","id":id,"request_id":"withdraw-current","expected_revision":4,"action":"withdraw","reason":"Withdraw the superseded contribution"})).unwrap();
    assert_eq!(
        mutate(&mut f.work.store, &a, &withdraw).unwrap()["contribution"]["state"],
        "withdrawn"
    );
    let history = f
        .work
        .store
        .query_repository_knowledge(&b, &project, &repository, None, &get)
        .unwrap();
    assert_eq!(history["history_total"], 5);
    assert_eq!(history["history"][0]["actor"], a.actor_id());
    assert_eq!(history["history"][2]["actor"], b.actor_id());
    let mut invalid = serde_json::to_value(&create).unwrap();
    invalid["request_id"] = json!("invalid-source-node");
    invalid["draft"]["source"] = json!("foreign-node");
    assert!(
        mutate(
            &mut f.work.store,
            &a,
            &serde_json::from_value(invalid).unwrap()
        )
        .is_err()
    );
    f.work
        .store
        .revoke_key(&f.work.ctx, a.credential_id())
        .unwrap();
    assert!(matches!(
        mutate(&mut f.work.store, &a, &create),
        Err(PlatformError::Unauthorized)
    ));
    drop(f.work.store);
    let mut restarted = PlatformStore::connect(&f.work.config, &f.work.namespace).unwrap();
    assert_eq!(
        restarted
            .query_repository_knowledge(&b, &project, &repository, None, &get)
            .unwrap()["history_total"],
        5
    );
    f.sql
        .execute(
            "UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id=$1",
            &[&f.connection],
        )
        .unwrap();
    assert!(matches!(
        mutate(&mut restarted, &b, &create),
        Err(PlatformError::NotFound)
    ));
    std::fs::remove_dir_all(&f.root).unwrap();
}
