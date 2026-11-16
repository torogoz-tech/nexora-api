#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::{KnowledgeContributionInput, MemberRole};

#[test]
#[ignore = "requires PostgreSQL"]
fn markdown_documents_preserve_images_and_versions_with_current_scope_permissions() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("markdown_dev", MemberRole::Developer);
    let reader = f.member("markdown_reader", MemberRole::Developer);
    let lead = f.member("markdown_lead", MemberRole::TechnicalLead);
    let project = f.work.project.id.clone();
    let bytes = b"\x89PNG\r\n\x1a\nfixture";
    let image = f
        .work
        .store
        .upload_knowledge_image(&developer, Some(&project), "architecture.png", bytes)
        .unwrap();
    let image_id = image["id"].as_str().unwrap();
    assert_eq!(
        f.work
            .store
            .knowledge_image_content(&reader, image_id)
            .unwrap(),
        bytes
    );
    let body = format!(
        "# Architecture\n![Diagram]({})\n\n```mermaid\nflowchart LR\nA-->B\n```",
        image["url"].as_str().unwrap()
    );
    let mut input = KnowledgeContributionInput {
        project_id: Some(project.clone()),
        kind: "document".into(),
        title: "Architecture".into(),
        body: body.clone(),
        filename: None,
        data_base64: None,
        expected_revision: None,
    };
    let saved = f
        .work
        .store
        .write_knowledge_contribution(&developer, None, &input, None)
        .unwrap();
    assert_eq!(saved["status"], "ready");
    assert_eq!(saved["body"], body);
    let id = saved["id"].as_str().unwrap();
    input.body = "# Revised architecture".into();
    input.expected_revision = Some(1);
    assert!(
        f.work
            .store
            .write_knowledge_contribution(&developer, Some(id), &input, None)
            .is_err()
    );
    assert_eq!(
        f.work
            .store
            .write_knowledge_contribution(&lead, Some(id), &input, None)
            .unwrap()["revision"],
        2
    );
    assert_eq!(
        f.work
            .store
            .knowledge_contribution(&reader, id, Some(1))
            .unwrap()["body"],
        body
    );
    assert_eq!(
        f.work
            .store
            .knowledge_image_content(&reader, image_id)
            .unwrap(),
        bytes
    );
    f.sql
        .execute(
            "DELETE FROM actor_grants WHERE actor_id=$1 AND project_id=$2",
            &[&reader.actor_id(), &project],
        )
        .unwrap();
    assert!(
        f.work
            .store
            .knowledge_image_content(&reader, image_id)
            .is_err()
    );
    let organization = f
        .work
        .store
        .upload_knowledge_image(&developer, None, "shared.png", bytes)
        .unwrap();
    assert_eq!(
        f.work
            .store
            .knowledge_image_content(&reader, organization["id"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    assert!(
        f.work
            .store
            .upload_knowledge_image(&developer, None, "bad.svg", b"<svg/>")
            .is_err()
    );
}
