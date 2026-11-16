#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/published_repository_fixture.rs"]
mod published_repository_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::*;
use published_repository_fixture::PublishedRepositoryFixture;
use published_repository_fixture::work_item_fixture;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; mixed Git/local manifests, exact evidence and context freshness"]
fn mixed_context_preserves_git_checksums_and_records_real_local_provenance() {
    let mut local = LocalSnapshotFixture::new();
    let mut git = PublishedRepositoryFixture::new();
    let project = git.work.project.id.clone();
    let ctx = git.work.ctx.clone();
    let input: LocalSnapshotAdoption = serde_json::from_value(json!({"name":"Imported context fixture","root":local.root,
        "runtime_tenant_id":local.header.tenant_id.0,"snapshot_id":local.header.id.0,
        "expected_payload_hash":local.header.payload_hash,"reason":"Adopt alongside an independently authorized Git source"})).unwrap();
    let canonical = json!({"project_id":project,"root_digest":local.header.repository_id.0,
        "runtime_tenant_id":input.runtime_tenant_id,"snapshot":input.snapshot_id,"hash":input.expected_payload_hash});
    let adopted = git
        .work
        .store
        .request(
            &ctx,
            "local_snapshot.adopt",
            "adopt-context",
            &canonical,
            |store| store.adopt_local_snapshot(&ctx, &project, &input),
        )
        .unwrap();
    let reader = git.contributor("Mixed source reader", false);
    let read = git
        .work
        .store
        .query_repository_code(
            &reader,
            &project,
            &adopted.repository_id,
            None,
            &nexora_runtime::KnowledgeQuery::Overview {
                offset: 0,
                limit: 10,
            },
        )
        .unwrap();
    assert_eq!(read["snapshot"], local.header.id.0);
    let manifest = git
        .work
        .store
        .create_project_context_manifest(
            &ctx,
            &project,
            &ProjectContextManifestCreate {
                name: "Mixed verified context".into(),
                repository_ids: vec![
                    adopted.repository_id.clone(),
                    git.binding.repository_id.clone(),
                ],
                reason: "Capture each source with its actual provenance".into(),
            },
        )
        .unwrap();
    let value = serde_json::to_value(&manifest).unwrap();
    let local_member = value["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["repository_id"] == adopted.repository_id)
        .unwrap();
    assert!(local_member["pinned_commit"].is_null());
    assert!(local_member["connection_id"].is_null());
    assert_eq!(
        local_member["local_origin"]["publication_id"],
        adopted.publication_id
    );
    let git_member = value["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["repository_id"] == git.binding.repository_id)
        .unwrap();
    assert_eq!(git_member["pinned_commit"], git.binding.pinned_commit);
    assert_eq!(git_member["connection_id"], git.connection);
    assert!(git_member.get("local_origin").is_none());
    let persisted: String = git
        .sql
        .query_one(
            "SELECT checksum FROM project_context_manifests WHERE id=$1",
            &[&manifest.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(persisted, manifest.checksum);
    assert!(
        !git.work
            .store
            .project_context_manifest_status(&ctx, &project, &manifest.id)
            .unwrap()
            .needs_review
    );
    let workflow = git.work.store.create_workflow(&ctx,&project,&serde_json::from_value(json!({
        "name":"Mixed source evidence","purpose":"Require fresh authorized source context","resource_kinds":["requirement"],
        "graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],
            "transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"evidence_valid"}]}]}})).unwrap()).unwrap();
    let flow = git
        .work
        .store
        .publish_workflow(
            &ctx,
            &project,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Verify exact source references".into(),
            },
        )
        .unwrap();
    let content = json!({"kind":"requirement","body":{"problem":"Source origins differ","objective":"Retain verified source evidence",
        "scope":"Public fixture functions","business_actors":["Developer"],"acceptance_criteria":["Both sources retain their true provenance"],
        "facts":["Each fixture declares a function"],"source":"External author claim; anchors prove existence only"}});
    let request: ProjectDocumentCreate = serde_json::from_value(json!({"code":"REQ-LOCAL","title":"Verified mixed evidence",
        "content":content,"origin":"observed_code","workflow_version_id":flow.id,"context_manifest_id":manifest.id,
        "evidence":[{"repository_id":adopted.repository_id,"path":"source.js","start_line":1,"line_count":1},
            {"repository_id":git.binding.repository_id,"path":"source.js","start_line":1,"line_count":1}],"reason":"Document independently verified references"})).unwrap();
    let doc = git
        .work
        .store
        .create_project_document(&ctx, &project, &request)
        .unwrap();
    let version = git
        .work
        .store
        .document_version(&ctx, &project, &doc.id, &doc.latest_version_id)
        .unwrap();
    let evidence = serde_json::to_value(&version.evidence).unwrap();
    let local_evidence = evidence
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["repository_id"] == adopted.repository_id)
        .unwrap();
    assert!(local_evidence["pinned_commit"].is_null());
    assert_eq!(
        local_evidence["local_origin"]["source_id"],
        adopted.source_id
    );
    let frozen = git
        .work
        .store
        .export_project_document(&ctx, &project, &doc.id, &version.id, "json")
        .unwrap()
        .content;
    let markdown = git
        .work
        .store
        .export_project_document(&ctx, &project, &doc.id, &version.id, "markdown")
        .unwrap()
        .content;
    assert!(markdown.contains(&adopted.source_id));
    assert!(markdown.contains(&adopted.publication_id));
    assert!(markdown.contains(&git.binding.pinned_commit));
    let package = git
        .work
        .store
        .create_project_context_package(
            &ctx,
            &project,
            &ProjectContextPackageCreate {
                name: "Frozen mixed context".into(),
                context_manifest_id: Some(manifest.id.clone()),
                document_versions: vec![ContextDocumentReference {
                    document_id: doc.id.clone(),
                    version_id: version.id.clone(),
                }],
                task_ids: vec![],
                reason: "Keep exact claims and source provenance for this version".into(),
            },
        )
        .unwrap();
    assert!(!package.needs_review);
    let package_content = serde_json::to_value(&package.content).unwrap();
    let raw = local
        .runtime
        .query_header(&local.header.tenant_id, &local.header.repository_id, None)
        .unwrap();
    assert_eq!(raw.payload_hash, local.header.payload_hash);
    let revisions: i64 = local.sql.query_one("SELECT count(*) FROM nexora_runtime.knowledge_revisions WHERE tenant_id=$1 AND repository_id=$2", &[&raw.tenant_id.0,&raw.repository_id.0]).unwrap().get(0);
    assert_eq!(revisions, 1);
    std::fs::write(
        local.root.join("source.js"),
        "function retained_identity() { return 44; }\n",
    )
    .unwrap();
    let next = local
        .runtime
        .index(
            &nexora_indexer::Indexer::new(
                nexora_language::LanguageDefinition::load(std::path::Path::new(
                    "../../languages/javascript",
                ))
                .unwrap(),
            ),
            &local.root,
            local.header.tenant_id.clone(),
        )
        .unwrap();
    let next = local
        .runtime
        .query_header(
            &next.snapshot.tenant_id,
            &next.snapshot.repository_id,
            Some(&next.snapshot.id),
        )
        .unwrap();
    assert!(
        !git.work
            .store
            .project_context_manifest_status(&ctx, &project, &manifest.id)
            .unwrap()
            .needs_review,
        "an unattested raw runtime head must not change the selected context"
    );
    let mut publication = input.clone();
    publication.repository_id = Some(adopted.repository_id.clone());
    publication.expected_revision = Some(adopted.source_revision);
    publication.snapshot_id = next.id.0.clone();
    publication.expected_payload_hash = next.payload_hash.clone();
    let next_publication = git.work.store.request(&ctx,"local_snapshot.adopt","publish-local-context",
        &json!({"project_id":project,"repository_id":adopted.repository_id,"snapshot":next.id.0,"hash":next.payload_hash,"expected_revision":adopted.source_revision}),
        |store| store.adopt_local_snapshot(&ctx,&project,&publication)).unwrap();
    assert_eq!(
        git.work
            .store
            .project_context_manifest_status(&ctx, &project, &manifest.id)
            .unwrap()
            .changed_repository_ids,
        vec![adopted.repository_id.clone()]
    );
    assert!(
        git.work
            .store
            .document_version(&ctx, &project, &doc.id, &version.id)
            .unwrap()
            .needs_review
    );
    assert!(
        git.work
            .store
            .transition_document_version(
                &ctx,
                &project,
                &doc.id,
                &version.id,
                "review",
                &DocumentTransitionRequest {
                    approval_id: None,
                    expected_state_revision: 1,
                    reason: "Stale source cannot satisfy the flow".into()
                }
            )
            .is_err()
    );
    git.publish_revision(2);
    let changed = git
        .work
        .store
        .project_context_manifest_status(&ctx, &project, &manifest.id)
        .unwrap();
    let mut expected = vec![
        git.binding.repository_id.clone(),
        adopted.repository_id.clone(),
    ];
    expected.sort();
    assert_eq!(changed.changed_repository_ids, expected);
    let mut historical = request.clone();
    historical.code = "REQ-LOCAL-HISTORY".into();
    let historical = git
        .work
        .store
        .create_project_document(&ctx, &project, &historical)
        .unwrap();
    let historical = git
        .work
        .store
        .document_version(
            &ctx,
            &project,
            &historical.id,
            &historical.latest_version_id,
        )
        .unwrap();
    assert!(historical.needs_review);
    let captured = historical
        .evidence
        .iter()
        .find(|e| e.repository_id == adopted.repository_id)
        .unwrap();
    assert_eq!(
        captured.local_origin.as_ref().unwrap().publication_id,
        adopted.publication_id
    );
    let revoke = LocalSourceChange {
        expected_revision: next_publication.source_revision,
        reason: "Retire this source without altering frozen documents".into(),
    };
    git.work
        .store
        .request(
            &ctx,
            "local_source.revoke",
            "revoke-context",
            &json!({"project_id":project,"repository_id":adopted.repository_id,"change":revoke}),
            |store| store.revoke_local_source(&ctx, &project, &adopted.repository_id, &revoke),
        )
        .unwrap();
    let status = git
        .work
        .store
        .project_context_manifest_status(&ctx, &project, &manifest.id)
        .unwrap();
    assert!(status.needs_review);
    assert_eq!(
        status.unavailable_repository_ids,
        vec![adopted.repository_id.clone()]
    );
    assert_eq!(
        git.work
            .store
            .export_project_document(&ctx, &project, &doc.id, &version.id, "json")
            .unwrap()
            .content,
        frozen
    );
    let stale = git
        .work
        .store
        .get_project_context_package(&ctx, &project, &package.id)
        .unwrap();
    assert!(stale.needs_review);
    assert_eq!(stale.checksum, package.checksum);
    assert_eq!(
        serde_json::to_value(stale.content).unwrap(),
        package_content
    );
    std::fs::remove_dir_all(&local.root).unwrap();
    std::fs::remove_dir_all(&git.root).unwrap();
}
