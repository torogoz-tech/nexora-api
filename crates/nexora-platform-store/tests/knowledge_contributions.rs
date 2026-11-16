#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::{KnowledgeContributionInput, MemberRole};
#[test]
#[ignore = "requires PostgreSQL"]
fn developer_sources_and_syntheses_are_versioned_and_curated_edits_require_leadership() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("knowledge_dev", MemberRole::Developer);
    let lead = f.member("knowledge_lead", MemberRole::TechnicalLead);
    let mut input = KnowledgeContributionInput {
        project_id: Some(f.work.project.id.clone()),
        kind: "document".into(),
        title: "Architecture".into(),
        body: String::new(),
        filename: Some("architecture.md".into()),
        data_base64: None,
        expected_revision: None,
    };
    let saved = f
        .work
        .store
        .write_knowledge_contribution(&developer, None, &input, Some(b"# Original source"))
        .unwrap();
    let page = f
        .work
        .store
        .knowledge_contributions(
            &developer,
            input.project_id.as_deref(),
            "document",
            "Arch",
            None,
            false,
        )
        .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert!(
        f.work
            .store
            .knowledge_contributions(
                &developer,
                input.project_id.as_deref(),
                "document",
                "",
                None,
                true
            )
            .is_ok()
    );
    let id = saved["id"].as_str().unwrap();
    assert_eq!(saved["status"], "queued");
    let claimed = f.work.store.claim_knowledge_synthesis().unwrap().unwrap();
    assert_eq!(claimed["id"], id);
    assert!(f.work.store.claim_knowledge_synthesis().unwrap().is_none());
    f.work
        .store
        .finish_knowledge_synthesis(id, 1, Some("First synthesis"))
        .unwrap();
    input.expected_revision = Some(2);
    input.filename = None;
    input.body = "Edited synthesis".into();
    assert!(
        f.work
            .store
            .write_knowledge_contribution(&developer, Some(id), &input, None)
            .is_err()
    );
    let edited = f
        .work
        .store
        .write_knowledge_contribution(&lead, Some(id), &input, None)
        .unwrap();
    assert_eq!(edited["revision"], 3);
    input.expected_revision = Some(3);
    input.filename = Some("architecture.md".into());
    let replaced = f
        .work
        .store
        .write_knowledge_contribution(&developer, Some(id), &input, Some(b"# Second source"))
        .unwrap();
    assert_eq!(replaced["source_revision"], 2);
    assert_eq!(
        f.work
            .store
            .knowledge_contribution_file(&developer, id, Some(2))
            .unwrap(),
        b"# Original source"
    );
    assert_eq!(
        f.work
            .store
            .knowledge_contribution_file(&developer, id, None)
            .unwrap(),
        b"# Second source"
    );
    let old = f
        .work
        .store
        .knowledge_contribution(&developer, id, Some(2))
        .unwrap();
    assert_eq!(old["body"], "First synthesis");
    assert_eq!(old["source_revision"], 1);
    f.work.store.claim_knowledge_synthesis().unwrap().unwrap();
    f.work
        .store
        .finish_knowledge_synthesis(id, 1, Some("Stale result"))
        .unwrap();
    assert_eq!(
        f.work
            .store
            .knowledge_contribution(&developer, id, None)
            .unwrap()["status"],
        "processing"
    );
    f.work
        .store
        .finish_knowledge_synthesis(id, 2, Some("Second synthesis"))
        .unwrap();
    let current = f
        .work
        .store
        .knowledge_contribution(&developer, id, None)
        .unwrap();
    assert_eq!(current["revision"], 5);
    assert_eq!(current["body"], "Second synthesis");
    input.kind = "technical_debt".into();
    input.filename = None;
    input.expected_revision = None;
    input.body = "Remove duplicate persistence".into();
    assert!(
        f.work
            .store
            .write_knowledge_contribution(&developer, None, &input, None)
            .is_ok()
    );
    input.project_id = None;
    assert!(
        f.work
            .store
            .write_knowledge_contribution(&developer, None, &input, None)
            .is_ok()
    );
}
#[test]
#[ignore = "requires PostgreSQL"]
fn validated_profiles_can_be_installed_by_developers_without_overwriting_versions() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("profile_dev", MemberRole::Developer);
    let package = nexora_language::AnalysisProfilePackage::from_catalog(
        std::path::Path::new("../../languages"),
        "css".into(),
        "team-css".into(),
        "1.0.0".into(),
        "Team CSS".into(),
    )
    .unwrap();
    let installed = f
        .work
        .store
        .import_analysis_profile(&developer, &package)
        .unwrap();
    let id = installed["id"].as_str().unwrap();
    let exported = f.work.store.analysis_profile(&developer, id, true).unwrap();
    assert_eq!(exported["key"], "team-css");
    let mut changed = package.clone();
    changed.name = "Different content".into();
    assert!(
        f.work
            .store
            .import_analysis_profile(&developer, &changed)
            .is_err()
    );
    changed.version = "1.0.1".into();
    assert!(
        f.work
            .store
            .import_analysis_profile(&developer, &changed)
            .is_ok()
    );
}
#[test]
#[ignore = "requires PostgreSQL"]
fn synthesis_does_not_start_after_uploader_is_archived() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("archived_upload", MemberRole::Developer);
    let input = KnowledgeContributionInput {
        project_id: None,
        kind: "document".into(),
        title: "Manual".into(),
        body: String::new(),
        filename: Some("manual.txt".into()),
        data_base64: None,
        expected_revision: None,
    };
    f.work
        .store
        .write_knowledge_contribution(&developer, None, &input, Some(b"Manual"))
        .unwrap();
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&developer.actor_id()],
        )
        .unwrap();
    assert!(f.work.store.claim_knowledge_synthesis().unwrap().is_none());
}

#[test]
#[ignore = "requires PostgreSQL"]
fn contributions_have_no_daily_intake_quota() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("unlimited_knowledge_dev", MemberRole::Developer);
    for index in 0..21 {
        for kind in ["document", "technical_debt"] {
            let input = KnowledgeContributionInput {
                project_id: Some(f.work.project.id.clone()),
                kind: kind.into(),
                title: format!("Contribution {kind} {index}"),
                body: "Observed technical debt".into(),
                filename: (kind == "document").then(|| "source.md".into()),
                data_base64: None,
                expected_revision: None,
            };
            let bytes = (kind == "document").then_some(b"# Source".as_slice());
            let saved = f
                .work
                .store
                .write_knowledge_contribution(&developer, None, &input, bytes)
                .unwrap();
            assert!(saved["id"].is_string());
        }
    }
}

#[test]
#[ignore = "requires PostgreSQL"]
fn ready_sources_enter_context_automatically_and_replacement_hides_old_text() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("retrieval_dev", MemberRole::Developer);
    let mut input = KnowledgeContributionInput {
        project_id: Some(f.work.project.id.clone()),
        kind: "document".into(),
        title: "Banking documentation".into(),
        body: String::new(),
        filename: Some("banking.md".into()),
        data_base64: None,
        expected_revision: None,
    };
    let saved = f
        .work
        .store
        .write_knowledge_contribution(&developer, None, &input, Some(b"# Original"))
        .unwrap();
    let id = saved["id"].as_str().unwrap();
    assert!(
        f.work
            .store
            .knowledge_context_search(&developer, input.project_id.as_deref(), "reconciliation")
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    f.work.store.claim_knowledge_synthesis().unwrap().unwrap();
    f.work
        .store
        .finish_knowledge_synthesis(
            id,
            1,
            Some("Bank reconciliation requires an idempotency key."),
        )
        .unwrap();
    let context = f
        .work
        .store
        .knowledge_context_search(&developer, input.project_id.as_deref(), "reconciliation")
        .unwrap();
    assert_eq!(context["items"][0]["id"], id);
    assert_eq!(context["items"][0]["source_revision"], 1);
    assert_eq!(context["items"][0]["revision"], 2);
    assert_eq!(context["items"][0]["untrusted_content"], true);
    input.expected_revision = Some(2);
    f.work
        .store
        .write_knowledge_contribution(&developer, Some(id), &input, Some(b"# Replacement"))
        .unwrap();
    assert!(
        f.work
            .store
            .knowledge_context_search(&developer, input.project_id.as_deref(), "reconciliation")
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    f.work.store.claim_knowledge_synthesis().unwrap().unwrap();
    f.work
        .store
        .finish_knowledge_synthesis(id, 2, Some("Settlement requires unique references."))
        .unwrap();
    assert_eq!(
        f.work
            .store
            .knowledge_context_search(&developer, input.project_id.as_deref(), "settlement")
            .unwrap()["items"][0]["source_revision"],
        2
    );
    input.project_id = None;
    input.kind = "technical_debt".into();
    input.title = "Global settlement debt".into();
    input.body = "Settlement migration".into();
    input.filename = None;
    input.expected_revision = None;
    f.work
        .store
        .write_knowledge_contribution(&developer, None, &input, None)
        .unwrap();
    assert_eq!(
        f.work
            .store
            .knowledge_context_search(&developer, Some(&f.work.project.id), "settlement")
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let issued = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            developer.actor_id(),
            "Project context only",
            nexora_platform::KeyScope {
                capabilities: vec![
                    nexora_platform::Capability::DocumentsRead,
                    nexora_platform::Capability::OrganizationKnowledgeRead,
                ],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&issued.key).unwrap();
    let scoped = f
        .work
        .store
        .knowledge_context_search(&api, Some(&f.work.project.id), "settlement")
        .unwrap();
    assert_eq!(scoped["organization_included"], false);
    assert_eq!(scoped["items"].as_array().unwrap().len(), 1);
    assert!(
        f.work
            .store
            .knowledge_context_search(&api, None, "settlement")
            .is_err()
    );
    f.sql.execute("DELETE FROM actor_grants WHERE actor_id=$1 AND capability='organization-knowledge:read'",&[&developer.actor_id()]).unwrap();
    let restricted = f
        .work
        .store
        .knowledge_context_search(&developer, Some(&f.work.project.id), "settlement")
        .unwrap();
    assert_eq!(restricted["organization_included"], false);
    assert_eq!(restricted["items"].as_array().unwrap().len(), 1);
    assert!(
        f.work
            .store
            .knowledge_context_search(&developer, None, "settlement")
            .is_err()
    );
    f.sql
        .execute(
            "DELETE FROM actor_grants WHERE actor_id=$1 AND capability='documents:read'",
            &[&developer.actor_id()],
        )
        .unwrap();
    assert!(
        f.work
            .store
            .knowledge_context_search(&developer, Some(&f.work.project.id), "settlement")
            .is_err()
    );
}
