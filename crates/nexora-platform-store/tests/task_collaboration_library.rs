#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;

#[test]
#[ignore = "requires PostgreSQL; documents follow task scope and preserve author history"]
fn files_comments_filters_and_archived_creators_keep_current_authority() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(None, None);
    let file = f
        .work
        .store
        .upload_task_document(
            &f.developer,
            &task.id,
            "specification.pdf",
            b"%PDF-1.7 example",
        )
        .unwrap();
    let id = file["id"].as_str().unwrap();
    let library = f
        .work
        .store
        .task_document_page(&f.other, None, None, "specification", None)
        .unwrap();
    assert_eq!(library["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        f.work.store.task_document_content(&f.other, id).unwrap().1,
        b"%PDF-1.7 example"
    );
    assert!(
        f.work
            .store
            .upload_task_document(&f.developer, &task.id, "../evil", b"data")
            .is_err()
    );
    let comment = f
        .work
        .store
        .add_pool_task_comment(
            &f.developer,
            &task.id,
            &TaskCommentCreate {
                body: "Buen trabajo 👍 [sticker:celebrate]".into(),
            },
        )
        .unwrap();
    assert!(comment.body.contains('👍'));
    let comments = f
        .work
        .store
        .pool_task_comments(&f.other, &task.id, None, 20)
        .unwrap();
    assert_eq!(comments.items[0].actor_name.as_deref(), Some("developer"));
    f.work
        .store
        .change_member_archived(&f.work.ctx, f.manager.actor_id(), true, 0)
        .unwrap();
    let page = f
        .work
        .store
        .visible_tasks(
            &f.work.ctx,
            &VisibleTaskQuery {
                project_id: Some("none".into()),
                assignee_id: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].created_by_name.as_deref(), Some("manager"));
    let empty = f
        .work
        .store
        .visible_tasks(
            &f.work.ctx,
            &VisibleTaskQuery {
                assignee_id: Some(f.developer.actor_id().into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(empty.total, 0);
    // A document of a project task invisible to the developer cannot leak via library or content.
    let hidden = f.work.task("Private to creator");
    let private = f
        .work
        .store
        .upload_task_document(&f.work.ctx, &hidden.id, "internal.docx", b"PK data")
        .unwrap();
    assert!(
        f.work
            .store
            .task_document_content(&f.developer, private["id"].as_str().unwrap())
            .is_err()
    );
    let list = f
        .work
        .store
        .task_document_page(&f.developer, None, Some(&f.work.project.id), "", None)
        .unwrap();
    assert!(list["items"].as_array().unwrap().is_empty());
}

#[test]
#[ignore = "requires PostgreSQL; shared knowledge role rules and immutable revisions"]
fn general_knowledge_is_readable_to_team_but_only_coordinators_can_write() {
    let mut f = TaskPoolFixture::new(false);
    let input = OrganizationDocumentWrite {
        kind: "best_practice".into(),
        title: "Testing policy".into(),
        body: "Use focused regression tests.".into(),
        expected_revision: None,
    };
    assert!(matches!(
        f.work
            .store
            .organization_knowledge_write(&f.developer, None, &input),
        Err(PlatformError::Forbidden)
    ));
    let first = f
        .work
        .store
        .organization_knowledge_write(&f.manager, None, &input)
        .unwrap();
    let id = first["id"].as_str().unwrap();
    assert_eq!(
        f.work
            .store
            .organization_knowledge_page(&f.developer, "best_practice", "Testing", None)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut change = input.clone();
    change.expected_revision = Some(1);
    change.body = "Updated testing policy".into();
    let latest = f
        .work
        .store
        .organization_knowledge_write(&f.other, Some(id), &change)
        .unwrap();
    assert_eq!(latest["revision"], 2);
    assert_eq!(
        f.work
            .store
            .organization_knowledge_get(&f.developer, id, Some(1))
            .unwrap()["body"],
        input.body
    );
    assert!(matches!(
        f.work
            .store
            .organization_knowledge_write(&f.manager, Some(id), &change),
        Err(PlatformError::Conflict(_))
    ));
}

#[test]
#[ignore = "requires PostgreSQL; filters must apply before the task cursor and page size"]
fn project_and_assignee_filters_are_composed_before_pagination() {
    let mut f = TaskPoolFixture::new(false);
    for _ in 0..23 {
        f.create(None, None);
    }
    let wanted = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let query = VisibleTaskQuery {
        project_id: Some(f.work.project.id.clone()),
        assignee_id: Some(f.developer.actor_id().into()),
        limit: 20,
        ..Default::default()
    };
    let page = f.work.store.visible_tasks(&f.work.ctx, &query).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].task.id, wanted.id);
    assert!(page.next_after.is_none());
    let unassigned = f
        .work
        .store
        .visible_tasks(
            &f.work.ctx,
            &VisibleTaskQuery {
                project_id: Some("none".into()),
                assignee_id: Some("none".into()),
                limit: 20,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(unassigned.total, 23);
    assert_eq!(unassigned.items.len(), 20);
    assert!(unassigned.next_after.is_some());
    let next = f
        .work
        .store
        .visible_tasks(
            &f.work.ctx,
            &VisibleTaskQuery {
                project_id: Some("none".into()),
                assignee_id: Some("none".into()),
                limit: 20,
                after: unassigned.next_after,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(next.total, 23);
    assert_eq!(next.items.len(), 3);
    assert!(next.next_after.is_none());
}

#[test]
#[ignore = "requires PostgreSQL; MCP organization access must remain explicitly delegated"]
fn general_knowledge_mcp_scope_is_attenuated() {
    let mut f = TaskPoolFixture::new(false);
    let input = PersonalMcpKeyCreate {
        name: "Project assistant".into(),
        project_ids: vec![f.work.project.id.clone()],
        read_only: false,
        include_global: false,
        expires_in_seconds: 3600,
    };
    let project_key = f
        .work
        .store
        .issue_personal_mcp_key(&f.other, &input)
        .unwrap();
    let project_ctx = f.work.store.authenticate(&project_key.key).unwrap();
    assert!(matches!(
        f.work
            .store
            .organization_knowledge_page(&project_ctx, "document", "", None),
        Err(PlatformError::Forbidden)
    ));
    let global_key = f
        .work
        .store
        .issue_personal_mcp_key(
            &f.other,
            &PersonalMcpKeyCreate {
                include_global: true,
                project_ids: vec![],
                ..input
            },
        )
        .unwrap();
    let global_ctx = f.work.store.authenticate(&global_key.key).unwrap();
    let write = OrganizationDocumentWrite {
        kind: "document".into(),
        title: "Shared".into(),
        body: "Organization policy".into(),
        expected_revision: None,
    };
    assert!(
        f.work
            .store
            .organization_knowledge_write(&global_ctx, None, &write)
            .is_ok()
    );
}

#[test]
#[ignore = "requires PostgreSQL; technical lead reads technical catalogs without configuration writes"]
fn technical_lead_reads_catalogs_and_team_ratings_without_self_access() {
    let mut f = TaskPoolFixture::new(false);
    assert!(f.work.store.git_connections(&f.other).is_ok());
    assert!(f.work.store.catalog_task_types(&f.other, None, 20).is_ok());
    assert!(f.work.store.analysis_profiles(&f.other, None, 20).is_ok());
    assert!(f.work.store.oidc_providers(&f.other).is_err());
    assert!(
        f.other
            .require(Capability::InstallationManage, None)
            .is_err()
    );
    assert!(
        f.other
            .require(Capability::ConnectionsManage, None)
            .is_err()
    );
    assert!(
        f.work
            .store
            .member_ratings(&f.other, f.developer.actor_id(), 2026)
            .is_ok()
    );
    assert!(matches!(
        f.work
            .store
            .member_ratings(&f.other, f.other.actor_id(), 2026),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.work
            .store
            .member_ratings(&f.developer, f.other.actor_id(), 2026),
        Err(PlatformError::Forbidden)
    ));
    let directory = f.work.store.rating_directory(&f.other, None, 20).unwrap();
    assert_eq!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == f.developer.actor_id())
            .unwrap()["can_view_evaluations"],
        true
    );
    assert_eq!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == f.other.actor_id())
            .unwrap()["can_view_evaluations"],
        false
    );
}

#[test]
#[ignore = "requires PostgreSQL; technical lead projects and member scope permissions"]
fn technical_lead_views_projects_and_only_changes_member_project_scope() {
    let mut f = TaskPoolFixture::new(false);
    assert!(
        f.work
            .store
            .project_profile(&f.other, &f.work.project.id, None)
            .is_ok()
    );
    assert!(f.other.require(Capability::ProjectsWrite, None).is_err());
    assert!(
        f.other
            .require(Capability::ProjectsWrite, Some(&f.work.project.id))
            .is_err()
    );
    assert!(f.work.store.organization_members(&f.other).is_ok());
    let old = f
        .work
        .store
        .member_role(&f.other, f.developer.actor_id())
        .unwrap();
    assert!(matches!(
        f.work.store.change_member_role(
            &f.other,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: false,
                project_ids: old.project_ids.clone(),
                expected_revision: old.revision
            }
        ),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.work.store.change_member_role(
            &f.other,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: old.revision
            }
        ),
        Err(PlatformError::Forbidden)
    ));
    let outside = f
        .work
        .store
        .create_project(&f.work.ctx, "Outside lead scope", None)
        .unwrap();
    let initial = f
        .work
        .store
        .change_member_role(
            &f.work.ctx,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![outside.id.clone()],
                expected_revision: old.revision,
            },
        )
        .unwrap();
    let updated = f
        .work
        .store
        .change_member_role(
            &f.other,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![outside.id.clone(), f.work.project.id.clone()],
                expected_revision: initial.revision,
            },
        )
        .unwrap();
    assert_eq!(updated.project_ids.len(), 2);
    assert!(matches!(
        f.work.store.change_member_role(
            &f.other,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: updated.revision
            }
        ),
        Err(PlatformError::Forbidden)
    ));
}

#[test]
#[ignore = "requires PostgreSQL; comment images must become persistent task evidence"]
fn comment_images_are_claimed_by_the_task() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(None, None);
    let image = f
        .work
        .store
        .upload_task_evidence(&f.developer, "comment.png", b"\x89PNG\r\n\x1a\nfixture")
        .unwrap();
    let body = format!("**Evidence** ![image]({})", image["url"].as_str().unwrap());
    f.work
        .store
        .add_pool_task_comment(&f.developer, &task.id, &TaskCommentCreate { body })
        .unwrap();
    assert!(
        f.work
            .store
            .task_evidence_content(&f.other, image["id"].as_str().unwrap())
            .is_ok()
    );
    let other = f.create(None, None);
    let body = format!("![image]({})", image["url"].as_str().unwrap());
    assert!(
        f.work
            .store
            .add_pool_task_comment(&f.developer, &other.id, &TaskCommentCreate { body })
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL; only configuration administrators manage the shared sticker catalog"]
fn stickers_preserve_existing_comments_after_archival() {
    let mut f = TaskPoolFixture::new(false);
    let png = b"\x89PNG\r\n\x1a\nfixture";
    assert!(matches!(
        f.work
            .store
            .upload_organization_sticker(&f.developer, "No", png),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.work
            .store
            .upload_organization_sticker(&f.other, "No", png),
        Err(PlatformError::Forbidden)
    ));
    let sticker = f
        .work
        .store
        .upload_organization_sticker(&f.work.ctx, "Celebration", png)
        .unwrap();
    let id = sticker["id"].as_str().unwrap();
    assert_eq!(
        f.work.store.organization_stickers(&f.developer).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    f.work
        .store
        .archive_organization_sticker(&f.work.ctx, id)
        .unwrap();
    assert!(
        f.work.store.organization_stickers(&f.developer).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.work
            .store
            .organization_sticker_content(&f.developer, id)
            .unwrap(),
        png
    );
}

#[test]
#[ignore = "requires PostgreSQL; timeline omits comments and retains authoritative state timestamps"]
fn workflow_timeline_only_contains_execution_state_changes() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(None, None);
    assert!(
        f.work
            .store
            .task_workflow_timeline(&f.developer, &task.id, None)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    f.work
        .store
        .add_pool_task_comment(
            &f.developer,
            &task.id,
            &TaskCommentCreate {
                body: "A comment, not a state change".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .transition_pool_task(
            &f.developer,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: task.revision,
                ..Default::default()
            },
        )
        .unwrap();
    let timeline = f
        .work
        .store
        .task_workflow_timeline(&f.other, &task.id, None)
        .unwrap();
    let events = timeline["items"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["operation"], "task.taken");
    assert_eq!(events[0]["details"]["after"]["state_id"], "ready");
    assert_eq!(events[1]["details"]["after"]["state_id"], "done");
    assert!(events[1]["created_at"].as_str().is_some());
    assert!(events[1]["details"]["after"].get("context").is_none());
    let hidden = f.work.task("Hidden task");
    assert!(
        f.work
            .store
            .task_workflow_timeline(&f.developer, &hidden.id, None)
            .is_err()
    );
}
