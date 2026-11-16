#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::*;
use nexora_runtime::{KnowledgeCommand, KnowledgeQuery};
use serde_json::{Value, json};

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; seven source queries and knowledge over adopted legacy runtime IDs"]
fn adopted_source_reads_require_current_project_authority_and_explicit_attestation() {
    let mut f = LocalSnapshotFixture::new();
    let publication = f.adopt();
    let project = f.work.project.id.clone();
    let repository = publication.repository_id;
    let actor = f
        .work
        .store
        .create_actor(
            &f.work.ctx,
            ActorKind::Agent,
            "Local reader",
            "Project-scoped local source fixture",
        )
        .unwrap();
    let capabilities = vec![
        Capability::RepositoriesRead,
        Capability::KnowledgeRead,
        Capability::KnowledgeWrite,
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
            "Local source reader",
            KeyScope {
                capabilities,
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = f.work.store.authenticate(&key.key).unwrap();
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
        .clone();
    std::fs::remove_file(f.root.join("source.js")).unwrap();
    let queries = vec![
        KnowledgeQuery::Overview {
            offset: 0,
            limit: 10,
        },
        KnowledgeQuery::FindSymbol {
            name: "retained_identity".into(),
            offset: 0,
            limit: 10,
        },
        KnowledgeQuery::References {
            node: node.clone(),
            offset: 0,
            limit: 10,
        },
        KnowledgeQuery::Dependencies {
            node: node.clone(),
            offset: 0,
            limit: 10,
        },
        KnowledgeQuery::Traverse {
            node,
            depth: 2,
            limit: 10,
        },
        KnowledgeQuery::Search {
            query: "retained_identity".into(),
            offset: 0,
            limit: 10,
        },
        KnowledgeQuery::Context {
            path: "source.js".into(),
            start_line: 1,
            line_count: 1,
        },
    ];
    for query in &queries {
        let value = f
            .work
            .store
            .query_repository_code(&reader, &project, &repository, None, query)
            .unwrap();
        assert_eq!(value["snapshot"], f.header.id.0);
        assert_eq!(value["repository"], f.header.repository_id.0);
        assert_eq!(value["repository_id"], repository);
        assert!(value["commit"].is_null());
        assert_eq!(value["local_origin"]["source_id"], publication.source_id);
        assert!(
            f.work
                .store
                .query_repository_code(&reader, &project, &repository, Some("unattested"), query)
                .is_err()
        );
        assert!(
            f.work
                .store
                .query_repository_code(&reader, "foreign-project", &repository, None, query)
                .is_err()
        );
    }
    let list = KnowledgeCommand::List {
        status: None,
        node: None,
        offset: 0,
        limit: 10,
    };
    let knowledge = f
        .work
        .store
        .query_repository_knowledge(&reader, &project, &repository, None, &list)
        .unwrap();
    assert_eq!(knowledge["total"], 1);
    let pending = f
        .work
        .store
        .pending_repository_knowledge(&reader, &project, &repository, None, 0, 10)
        .unwrap();
    assert_eq!(pending["snapshot"], f.header.id.0);
    let command: KnowledgeCommand = serde_json::from_value(json!({"operation":"contribute","request_id":"local-new-claim",
        "draft":{"kind":"decision","statement":"The platform preserves this source identity.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]}})).unwrap();
    let canonical = json!({"project_id":project,"repository_id":repository,"snapshot":Value::Null,"command":command});
    let first = f
        .work
        .store
        .request(
            &reader,
            "knowledge.contribute",
            "local-new-claim",
            &canonical,
            |store| {
                store.query_repository_knowledge(&reader, &project, &repository, None, &command)
            },
        )
        .unwrap();
    let replay = f
        .work
        .store
        .request(
            &reader,
            "knowledge.contribute",
            "local-new-claim",
            &canonical,
            |store| {
                store.query_repository_knowledge(&reader, &project, &repository, None, &command)
            },
        )
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(first["contribution"]["actor"], reader.actor_id());
    let revisions: i64 = f.sql.query_one("SELECT count(*) FROM nexora_runtime.knowledge_revisions WHERE tenant_id=$1 AND repository_id=$2", &[&f.header.tenant_id.0,&f.header.repository_id.0]).unwrap().get(0);
    assert_eq!(revisions, 2);
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&f.work.ctx.actor_id()],
        )
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .query_repository_code(&reader, &project, &repository, None, &queries[0]),
        Err(PlatformError::NotFound)
    ));
    assert!(
        f.work
            .store
            .request(
                &reader,
                "knowledge.contribute",
                "local-new-claim",
                &canonical,
                |store| store.query_repository_knowledge(
                    &reader,
                    &project,
                    &repository,
                    None,
                    &command
                )
            )
            .is_err()
    );
    f.sql
        .execute(
            "UPDATE actors SET active=true WHERE id=$1",
            &[&f.work.ctx.actor_id()],
        )
        .unwrap();
    f.work
        .store
        .revoke_key(&f.work.ctx, &key.metadata.id)
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .query_repository_code(&reader, &project, &repository, None, &queries[0]),
        Err(PlatformError::Unauthorized)
    ));
    assert!(
        f.work
            .store
            .request(
                &reader,
                "knowledge.contribute",
                "local-new-claim",
                &canonical,
                |store| store.query_repository_knowledge(
                    &reader,
                    &project,
                    &repository,
                    None,
                    &command
                )
            )
            .is_err()
    );
    std::fs::remove_dir_all(&f.root).unwrap();
}
