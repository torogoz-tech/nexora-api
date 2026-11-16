#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::{DatabaseRuntime, KnowledgeQuery};
use serde_json::json;
use std::path::Path;
use work_item_fixture::WorkItemFixture;
use zeroize::Zeroizing;

fn publish_source(
    f: &mut WorkItemFixture,
    sql: &mut postgres::Client,
    runtime: &mut DatabaseRuntime,
    connection: &str,
    root: &Path,
    seed: u8,
) -> RepositoryBinding {
    let repository = f
        .store
        .register_repository(
            &f.ctx,
            &f.project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: format!("fixture/context-{seed}"),
            },
        )
        .unwrap();
    // This fixture exercises the legacy single-indexer publication path explicitly.
    f.store
        .change_repository_analysis_profile(
            &f.ctx,
            &f.project.id,
            &repository.id,
            &nexora_platform::RepositoryAnalysisProfileChange {
                expected_revision: 0,
                profile_version_id: None,
                reason: "Legacy publication fixture".into(),
                mode: None,
                profile_version_ids: None,
            },
        )
        .unwrap();
    // Isolated consent fixture, not a real GitLab connection. Runtime content is staged and published normally.
    sql.execute("UPDATE project_repositories SET remote_id=$1,remote_full_name=requested_full_name,connection_id=$2,authorization_status='authorized',revision=2 WHERE id=$3",&[&(100+u16::from(seed)).to_string(),&connection,&repository.id]).unwrap();
    publish_revision(f, runtime, &repository.id, root, seed)
}
fn publish_revision(
    f: &mut WorkItemFixture,
    runtime: &mut DatabaseRuntime,
    repository: &str,
    root: &Path,
    seed: u8,
) -> RepositoryBinding {
    f.store
        .enqueue_repository_job(
            &f.ctx,
            &f.project.id,
            repository,
            RepositoryJobKind::Index,
            &RepositoryJobRequest {
                expected_revision: 2,
            },
        )
        .unwrap();
    let lease = f
        .store
        .claim_repository_job("context-publisher", 300)
        .unwrap()
        .unwrap();
    assert_eq!(lease.job.repository_id, repository);
    let commit = char::from(b'a' + seed).to_string().repeat(40);
    let scope = f.store.pin_repository_job(&lease, &commit).unwrap();
    std::fs::write(
        root.join("source.js"),
        format!("function source_{seed}() {{ return {seed}; }}\n"),
    )
    .unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let staged = runtime
        .stage_repository(
            &indexer,
            root,
            TenantId(scope.tenant_id.clone()),
            RepositoryId(scope.repository_id.clone()),
        )
        .unwrap();
    let header = runtime
        .query_header(
            &TenantId(scope.tenant_id),
            &RepositoryId(scope.repository_id),
            Some(&staged.snapshot.id),
        )
        .unwrap();
    f.store.complete_repository_job(&lease, &header).unwrap()
}

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; explicit multirepository context, verified code anchors and source freshness"]
fn business_versions_pin_real_multirepository_sources_and_require_review_after_change() {
    let mut f = WorkItemFixture::new();
    f.task("External actors document source context");
    let password = Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')",&[&connection,&f.ctx.actor_id()]).unwrap();
    sql.execute(
        "INSERT INTO git_connection_projects(connection_id,project_id,granted_by) VALUES($1,$2,$3)",
        &[&connection, &f.project.id, &f.ctx.actor_id()],
    )
    .unwrap();
    let actor = f
        .store
        .create_actor(
            &f.ctx,
            ActorKind::Agent,
            "External context contributor",
            "Provide source-backed business documentation",
        )
        .unwrap();
    let capabilities = vec![
        Capability::DocumentsRead,
        Capability::DocumentsWrite,
        Capability::DocumentsPublish,
        Capability::RepositoriesRead,
        Capability::KnowledgeRead,
    ];
    for capability in &capabilities {
        f.store
            .grant(&f.ctx, &actor.id, *capability, Some(&f.project.id))
            .unwrap();
    }
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Context contributor",
            KeyScope {
                capabilities,
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let contributor = f.store.authenticate(&key.key).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(SecretGenerator::id("nexora-context-fixture").unwrap());
    std::fs::create_dir(&root).unwrap();
    let mut runtime = DatabaseRuntime::connect(&f.config).unwrap();
    let first = publish_source(&mut f, &mut sql, &mut runtime, &connection, &root, 1);
    let second = publish_source(&mut f, &mut sql, &mut runtime, &connection, &root, 2);
    assert_ne!(first.pinned_commit, second.pinned_commit);
    let input=serde_json::from_value(json!({"name":"Explicit business context","repository_ids":[second.repository_id,first.repository_id],"reason":"Document two independently pinned repositories"})).unwrap();
    let manifest = f
        .store
        .create_project_context_manifest(&contributor, &f.project.id, &input)
        .unwrap();
    assert_eq!(manifest.members.len(), 2);
    assert_eq!(manifest.created_by, actor.id);
    assert!(manifest.members[0].repository_id < manifest.members[1].repository_id);
    assert_eq!(manifest.checksum.len(), 64);
    let selected = manifest
        .members
        .iter()
        .find(|member| member.repository_id == first.repository_id)
        .unwrap();
    assert_eq!(selected.snapshot_id, first.snapshot_id);
    assert_eq!(selected.payload_hash, first.payload_hash);
    assert_eq!(
        selected.pinned_commit.as_deref(),
        Some(first.pinned_commit.as_str())
    );
    assert!(
        !f.store
            .project_context_manifest_status(&contributor, &f.project.id, &manifest.id)
            .unwrap()
            .needs_review
    );
    assert!(f.store.create_project_context_manifest(&contributor,&f.project.id,&serde_json::from_value(json!({"name":"Duplicate","repository_ids":[first.repository_id,first.repository_id],"reason":"Duplicate members"})).unwrap()).is_err());
    assert!(f.store.create_project_context_manifest(&contributor,&f.project.id,&serde_json::from_value(json!({"name":"Foreign","repository_ids":["foreign-repository"],"reason":"Cannot forge membership"})).unwrap()).is_err());
    let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Verified source review","purpose":"Declarative verified evidence condition","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"},{"kind":"evidence_valid"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Require verified source anchors".into(),
            },
        )
        .unwrap();
    let content = json!({"kind":"requirement","body":{"problem":"Source context needs documentation","objective":"Record observations at independently pinned snapshots","scope":"Fixture functions","business_actors":["Developer"],"acceptance_criteria":["Source references are retrievable at the pinned snapshot"],"facts":["Functions return declared fixture constants"],"source":"External actor observation; source existence does not prove semantic entailment"}});
    let evidence = json!([{"repository_id":first.repository_id,"path":"source.js","start_line":1,"line_count":1},{"repository_id":second.repository_id,"path":"source.js","start_line":1,"line_count":1}]);
    let input=serde_json::from_value(json!({"code":"REQ-CONTEXT","title":"Observed source context","content":content,"origin":"observed_code","workflow_version_id":flow.id,"context_manifest_id":manifest.id,"evidence":evidence,"reason":"External author supplies referenced content"})).unwrap();
    let doc = f
        .store
        .create_project_document(&contributor, &f.project.id, &input)
        .unwrap();
    let version = f
        .store
        .document_version(&contributor, &f.project.id, &doc.id, &doc.latest_version_id)
        .unwrap();
    assert_eq!(version.created_by, actor.id);
    let value = serde_json::to_value(&version).unwrap();
    assert_eq!(value["context_manifest_id"], manifest.id);
    assert_eq!(value["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["evidence"][0]["context_checksum"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(value["needs_review"], false);
    let immutable_export = f
        .store
        .export_project_document(&contributor, &f.project.id, &doc.id, &version.id, "json")
        .unwrap()
        .content;
    let markdown = f
        .store
        .export_project_document(
            &contributor,
            &f.project.id,
            &doc.id,
            &version.id,
            "markdown",
        )
        .unwrap()
        .content;
    assert!(markdown.contains(&manifest.id));
    assert!(markdown.contains(&first.pinned_commit));
    assert!(markdown.contains(&second.pinned_commit));
    assert!(markdown.contains("source.js"));

    let reviewed = f
        .store
        .transition_document_version(
            &contributor,
            &f.project.id,
            &doc.id,
            &version.id,
            "review",
            &DocumentTransitionRequest {
                approval_id: None,
                expected_state_revision: 1,
                reason: "Review verified references".into(),
            },
        )
        .unwrap();
    f.store
        .publish_project_document(
            &contributor,
            &f.project.id,
            &doc.id,
            &ProjectDocumentPublish {
                expected_revision: doc.revision,
                version_id: version.id.clone(),
                expected_state_revision: reviewed.state_revision,
                reason: "Publish source-bound requirement".into(),
            },
        )
        .unwrap();
    let next = publish_revision(&mut f, &mut runtime, &first.repository_id, &root, 3);
    assert_ne!(first.snapshot_id, next.snapshot_id);
    assert!(
        f.store
            .project_context_manifest_status(&contributor, &f.project.id, &manifest.id)
            .unwrap()
            .needs_review
    );
    let old = f
        .store
        .get_project_context_manifest(&contributor, &f.project.id, &manifest.id)
        .unwrap();
    assert_eq!(old.checksum, manifest.checksum);
    assert_eq!(
        old.members
            .iter()
            .find(|member| member.repository_id == first.repository_id)
            .unwrap()
            .snapshot_id,
        first.snapshot_id
    );
    let historical = f
        .store
        .document_version(&contributor, &f.project.id, &doc.id, &version.id)
        .unwrap();
    assert_eq!(historical.checksum, version.checksum);
    assert_eq!(historical.content, version.content);
    let catalog = f
        .store
        .project_document_page(
            &contributor,
            &f.project.id,
            &serde_json::from_value(json!({"limit":10})).unwrap(),
        )
        .unwrap();
    assert!(
        catalog
            .items
            .iter()
            .find(|item| item.id == doc.id)
            .unwrap()
            .needs_review
    );

    assert_eq!(
        serde_json::to_value(&historical).unwrap()["needs_review"],
        true
    );
    assert_eq!(
        immutable_export,
        f.store
            .export_project_document(&contributor, &f.project.id, &doc.id, &version.id, "json")
            .unwrap()
            .content
    );

    let context = f
        .store
        .query_repository_code(
            &contributor,
            &f.project.id,
            &first.repository_id,
            Some(&first.snapshot_id),
            &KnowledgeQuery::Context {
                path: "source.js".into(),
                start_line: 1,
                line_count: 1,
            },
        )
        .unwrap();
    assert!(
        context["items"][0]["text"]
            .as_str()
            .unwrap()
            .contains("source_1")
    );
    let current = f
        .store
        .get_project_document(&contributor, &f.project.id, &doc.id)
        .unwrap();
    let input=serde_json::from_value(json!({"expected_revision":current.revision,"title":"Still using old context","content":content,"origin":"observed_code","workflow_version_id":flow.id,"context_manifest_id":manifest.id,"evidence":evidence,"reason":"Stale context is explicit"})).unwrap();
    let stale = f
        .store
        .revise_project_document(&contributor, &f.project.id, &doc.id, &input)
        .unwrap();
    assert!(
        f.store
            .transition_document_version(
                &contributor,
                &f.project.id,
                &doc.id,
                &stale.id,
                "review",
                &DocumentTransitionRequest {
                    approval_id: None,
                    expected_state_revision: 1,
                    reason: "Stale references must not satisfy evidence_valid".into()
                }
            )
            .is_err()
    );
    let invalid=serde_json::from_value(json!({"code":"INVALID-RANGE","title":"Reject missing code","content":content,"origin":"observed_code","workflow_version_id":flow.id,"context_manifest_id":manifest.id,"evidence":[{"repository_id":first.repository_id,"path":"source.js","start_line":999,"line_count":1}],"reason":"Out of source bounds"})).unwrap();
    assert!(
        f.store
            .create_project_document(&contributor, &f.project.id, &invalid)
            .is_err()
    );
    assert!(matches!(
        f.store
            .get_project_context_manifest(&contributor, "foreign-project", &manifest.id),
        Err(PlatformError::NotFound)
    ));
    drop(f.store);
    let mut restarted = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        restarted
            .get_project_context_manifest(&contributor, &f.project.id, &manifest.id)
            .unwrap()
            .checksum,
        manifest.checksum
    );
    let error = sql
        .execute(
            "UPDATE project_context_manifests SET name='tampered' WHERE id=$1",
            &[&manifest.id],
        )
        .unwrap_err();
    assert_eq!(error.code().map(|code| code.code()), Some("23514"));
    restarted.revoke_key(&f.ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        restarted.get_project_context_manifest(&contributor, &f.project.id, &manifest.id),
        Err(PlatformError::Unauthorized)
    ));
    std::fs::remove_dir_all(root).unwrap();
}
