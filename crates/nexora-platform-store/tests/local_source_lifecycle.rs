#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::*;
use nexora_runtime::{KnowledgeCommand, KnowledgeQuery};
use serde_json::{Value, json};
use std::path::Path;

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; data publication and explicit trust epochs govern history and frozen replies"]
fn publication_revocation_and_explicit_reauthorization_keep_data_and_trust_separate() {
    let mut f = LocalSnapshotFixture::new();
    let first = f.adopt();
    let ctx = f.work.ctx.clone();
    let project = f.work.project.id.clone();
    let repository = first.repository_id.clone();
    let query = KnowledgeQuery::Overview {
        offset: 0,
        limit: 10,
    };
    let command: KnowledgeCommand = serde_json::from_value(json!({"operation":"contribute","request_id":"before-new-publication",
        "draft":{"kind":"decision","statement":"Retain this acknowledged snapshot.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]}})).unwrap();
    let knowledge_input = json!({"project_id":project,"repository_id":repository,"snapshot":Value::Null,"command":command});
    let contribute = |store: &mut nexora_platform_store::PlatformStore| {
        store.request(
            &ctx,
            "knowledge.contribute",
            "before-new-publication",
            &knowledge_input,
            |store| store.query_repository_knowledge(&ctx, &project, &repository, None, &command),
        )
    };
    let acknowledgement = contribute(&mut f.work.store).unwrap();
    std::fs::write(
        f.root.join("source.js"),
        "function retained_identity() { return 43; }\n",
    )
    .unwrap();
    let next = f
        .runtime
        .index(
            &Indexer::new(
                LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap(),
            ),
            &f.root,
            f.header.tenant_id.clone(),
        )
        .unwrap();
    let next = f
        .runtime
        .query_header(
            &next.snapshot.tenant_id,
            &next.snapshot.repository_id,
            Some(&next.snapshot.id),
        )
        .unwrap();
    assert_ne!(next.id, f.header.id);
    assert_eq!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .unwrap()["snapshot"],
        f.header.id.0
    );
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, Some(&next.id.0), &query)
            .is_err()
    );
    let mut input: LocalSnapshotAdoption = serde_json::from_value(json!({"name":"Published local fixture","root":f.root,
        "runtime_tenant_id":next.tenant_id.0,"snapshot_id":next.id.0,"expected_payload_hash":next.payload_hash,
        "reason":"Select verified local publication","repository_id":repository,"expected_revision":first.source_revision})).unwrap();
    let canonical = json!({"project_id":project,"repository_id":repository,"root_digest":next.repository_id.0,
        "snapshot_id":input.snapshot_id,"expected_hash":input.expected_payload_hash,"expected_revision":input.expected_revision});
    let second = f
        .work
        .store
        .request(
            &ctx,
            "local_snapshot.adopt",
            "publish-next",
            &canonical,
            |store| store.adopt_local_snapshot(&ctx, &project, &input),
        )
        .unwrap();
    assert_eq!(second.source_id, first.source_id);
    assert_eq!(second.source_revision, 2);
    assert_eq!(second.authorization_revision, 1);
    assert_eq!(second.binding_revision, 2);
    assert_eq!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .unwrap()["snapshot"],
        next.id.0
    );
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, Some(&f.header.id.0), &query)
            .is_ok()
    );
    assert_eq!(contribute(&mut f.work.store).unwrap(), acknowledgement);
    assert!(
        f.work
            .store
            .request(
                &ctx,
                "local_snapshot.adopt",
                "stale-publication",
                &canonical,
                |store| store.adopt_local_snapshot(&ctx, &project, &input)
            )
            .is_err()
    );
    let change = LocalSourceChange {
        expected_revision: second.source_revision,
        reason: "Retire trust in this imported source".into(),
    };
    let revoke_input = json!({"project_id":project,"repository_id":repository,"change":change});
    f.sql.batch_execute("CREATE FUNCTION reject_local_lifecycle_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation IN ('local_source.revoked','local_source.reauthorized') THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='fixture lifecycle audit rejection'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_local_lifecycle_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_local_lifecycle_audit();").unwrap();
    assert!(
        f.work
            .store
            .request(
                &ctx,
                "local_source.revoke",
                "revoke-once",
                &revoke_input,
                |store| store.revoke_local_source(&ctx, &project, &repository, &change)
            )
            .is_err()
    );
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .is_ok()
    );
    f.sql
        .batch_execute("DROP TRIGGER reject_local_lifecycle_audit ON audit_events;")
        .unwrap();
    let revoked = f
        .work
        .store
        .request(
            &ctx,
            "local_source.revoke",
            "revoke-once",
            &revoke_input,
            |store| store.revoke_local_source(&ctx, &project, &repository, &change),
        )
        .unwrap();
    let replay = f
        .work
        .store
        .request(
            &ctx,
            "local_source.revoke",
            "revoke-once",
            &revoke_input,
            |store| store.revoke_local_source(&ctx, &project, &repository, &change),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&revoked).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    assert_eq!(revoked.state, LocalSourceState::Revoked);
    assert_eq!(revoked.authorization_revision, 2);
    assert_eq!(revoked.source_generation, 1);
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .is_err()
    );
    assert!(contribute(&mut f.work.store).is_err());
    let metadata = f
        .work
        .store
        .get_repository(&ctx, &project, &repository)
        .unwrap();
    let archived = f
        .work
        .store
        .change_repository(
            &ctx,
            &project,
            &repository,
            &RepositoryChange {
                expected_revision: metadata.revision,
                archived: true,
            },
        )
        .unwrap();
    f.work
        .store
        .change_repository(
            &ctx,
            &project,
            &repository,
            &RepositoryChange {
                expected_revision: archived.revision,
                archived: false,
            },
        )
        .unwrap();
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .is_err()
    );
    input.expected_revision = Some(revoked.source_revision);
    let reauthorize_input = json!({"project_id":project,"repository_id":repository,"snapshot":input.snapshot_id,
        "expected_revision":input.expected_revision,"root_digest":next.repository_id.0});
    assert!(
        f.work
            .store
            .request(
                &ctx,
                "local_snapshot.adopt",
                "cannot-auto-restore",
                &reauthorize_input,
                |store| store.adopt_local_snapshot(&ctx, &project, &input)
            )
            .is_err()
    );
    f.sql.batch_execute("CREATE TRIGGER reject_local_lifecycle_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_local_lifecycle_audit();").unwrap();
    assert!(
        f.work
            .store
            .request(
                &ctx,
                "local_source.reauthorize",
                "explicit-new-trust",
                &reauthorize_input,
                |store| store.reauthorize_local_source(&ctx, &project, &input)
            )
            .is_err()
    );
    assert_eq!(
        f.work
            .store
            .get_repository(&ctx, &project, &repository)
            .unwrap()
            .local_source
            .unwrap()
            .source_revision,
        revoked.source_revision
    );
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .is_err()
    );
    f.sql.batch_execute("DROP TRIGGER reject_local_lifecycle_audit ON audit_events; DROP FUNCTION reject_local_lifecycle_audit();").unwrap();
    let restored = f
        .work
        .store
        .request(
            &ctx,
            "local_source.reauthorize",
            "explicit-new-trust",
            &reauthorize_input,
            |store| store.reauthorize_local_source(&ctx, &project, &input),
        )
        .unwrap();
    assert_eq!(restored.source_id, first.source_id);
    assert_eq!(restored.repository_id, first.repository_id);
    assert_eq!(restored.source_revision, 4);
    assert_eq!(restored.authorization_revision, 3);
    assert_eq!(restored.source_generation, 2);
    assert_eq!(restored.binding_revision, 3);
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .is_ok()
    );
    assert!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, Some(&f.header.id.0), &query)
            .is_err()
    );
    assert!(
        contribute(&mut f.work.store).is_err(),
        "a new trust epoch must not revive old protected receipts"
    );
    input.snapshot_id = f.header.id.0.clone();
    input.expected_payload_hash = f.header.payload_hash.clone();
    input.expected_revision = Some(restored.source_revision);
    let historical = json!({"project_id":project,"repository_id":repository,"snapshot":input.snapshot_id,
        "expected_hash":input.expected_payload_hash,"expected_revision":input.expected_revision});
    f.work
        .store
        .request(
            &ctx,
            "local_snapshot.adopt",
            "reattest-historical",
            &historical,
            |store| store.adopt_local_snapshot(&ctx, &project, &input),
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .query_repository_code(&ctx, &project, &repository, None, &query)
            .unwrap()["snapshot"],
        f.header.id.0
    );
    assert!(contribute(&mut f.work.store).is_err());
    assert_eq!(
        f.runtime
            .query_header(&f.header.tenant_id, &f.header.repository_id, None)
            .unwrap()
            .id,
        next.id
    );
    let publications: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM local_snapshot_publications WHERE source_id=$1",
            &[&first.source_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(publications, 4);
    std::fs::remove_dir_all(&f.root).unwrap();
}
