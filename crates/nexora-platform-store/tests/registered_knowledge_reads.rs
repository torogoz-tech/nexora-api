#[path = "support/published_repository_fixture.rs"]
mod published_repository_fixture;
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::*;
use nexora_runtime::KnowledgeCommand;
use published_repository_fixture::PublishedRepositoryFixture;
use serde_json::json;
#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; registered knowledge reads retain live source/key authority"]
fn registered_knowledge_reads_are_bounded_and_keep_source_and_actor_authority() {
    let mut f = PublishedRepositoryFixture::new();
    let reader = f.contributor("Knowledge reader", false);
    let command:KnowledgeCommand=serde_json::from_value(json!({"operation":"contribute","request_id":"legacy-fixture-proposal","draft":{"kind":"decision","statement":"An existing local client declared this source decision.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]}})).unwrap();
    let seeded = f
        .runtime
        .knowledge(
            &TenantId(f.binding.runtime_tenant_id.clone()),
            &RepositoryId(f.binding.runtime_repository_id.clone()),
            Some(&SnapshotId(f.binding.snapshot_id.clone())),
            "legacy-fixture",
            &command,
        )
        .unwrap();
    let id = seeded["contribution"]["id"].as_str().unwrap();
    let list = KnowledgeCommand::List {
        status: None,
        node: None,
        offset: 0,
        limit: 1,
    };
    let page = f
        .work
        .store
        .query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &f.binding.repository_id,
            None,
            &list,
        )
        .unwrap();
    assert_eq!(page["project_id"], f.work.project.id);
    assert_eq!(page["repository_id"], f.binding.repository_id);
    assert_eq!(page["snapshot"], f.binding.snapshot_id);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["id"], id);
    let record = f
        .work
        .store
        .query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &f.binding.repository_id,
            None,
            &KnowledgeCommand::Get {
                id: id.into(),
                offset: 0,
                limit: 10,
            },
        )
        .unwrap();
    assert_eq!(record["contribution"]["id"], id);
    assert_eq!(record["contribution"]["actor"], "legacy-fixture");
    assert_eq!(record["history_total"], 1);
    let pending = f
        .work
        .store
        .pending_repository_knowledge(
            &reader,
            &f.work.project.id,
            &f.binding.repository_id,
            None,
            0,
            1,
        )
        .unwrap();
    assert_eq!(pending["project_id"], f.work.project.id);
    assert_eq!(pending["snapshot"], f.binding.snapshot_id);
    assert!(matches!(
        f.work.store.pending_repository_knowledge(
            &reader,
            &f.work.project.id,
            &f.binding.repository_id,
            None,
            0,
            0
        ),
        Err(PlatformError::Validation(_))
    ));
    assert!(matches!(
        f.work.store.query_repository_knowledge(
            &reader,
            "foreign-project",
            &f.binding.repository_id,
            None,
            &list
        ),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        f.work.store.query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &f.binding.repository_id,
            Some("foreign-snapshot"),
            &list
        ),
        Err(PlatformError::NotFound)
    ));
    let first = f.binding.clone();
    let next = f.publish_revision(2);
    assert_ne!(first.snapshot_id, next.snapshot_id);
    let historic = f
        .work
        .store
        .query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &first.repository_id,
            Some(&first.snapshot_id),
            &list,
        )
        .unwrap();
    assert_eq!(historic["snapshot"], first.snapshot_id);
    // Stage another snapshot without a successful platform publication: it grants no code/knowledge scope.
    std::fs::write(
        f.root.join("source.js"),
        "function unpublished() { return 3; }\n",
    )
    .unwrap();
    let indexer = nexora_indexer::Indexer::new(
        nexora_language::LanguageDefinition::load(std::path::Path::new(
            "../../languages/javascript",
        ))
        .unwrap(),
    );
    let staged = f
        .runtime
        .stage_repository(
            &indexer,
            &f.root,
            TenantId(first.runtime_tenant_id.clone()),
            RepositoryId(first.runtime_repository_id.clone()),
        )
        .unwrap();
    assert!(matches!(
        f.work.store.query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &first.repository_id,
            Some(&staged.snapshot.id.0),
            &list
        ),
        Err(PlatformError::NotFound)
    ));
    f.work
        .store
        .revoke_key(&f.work.ctx, reader.credential_id())
        .unwrap();
    assert!(matches!(
        f.work.store.query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &first.repository_id,
            None,
            &list
        ),
        Err(PlatformError::Unauthorized)
    ));
    let reader = f.contributor("Source revocation reader", false);
    f.sql
        .execute(
            "UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id=$1",
            &[&f.connection],
        )
        .unwrap();
    assert!(matches!(
        f.work.store.query_repository_knowledge(
            &reader,
            &f.work.project.id,
            &first.repository_id,
            None,
            &list
        ),
        Err(PlatformError::NotFound)
    ));
    drop(f.work.store);
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.work.config, &f.work.namespace).unwrap();
    assert!(matches!(
        restarted.query_repository_knowledge(
            &f.work.ctx,
            &f.work.project.id,
            &first.repository_id,
            None,
            &list
        ),
        Err(PlatformError::NotFound)
    ));
    std::fs::remove_dir_all(&f.root).unwrap();
}
