#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::*;
use serde_json::json;
#[test]
#[ignore = "requires PostgreSQL; human decisions, scoped roles, atomic task creation and feature reuse"]
fn governed_feature_maps_separate_proposal_decision_and_reuse() {
    let mut f = FeatureMapFixture::new();
    let input = f.proposal("Identity verification", true);
    let proposal = f
        .work
        .store
        .propose_feature_map(&f.pmo, &f.work.project.id, &input)
        .unwrap();
    assert_eq!(f.counts(), (0, 0, 0, 0, 0));
    let decision = FeatureMapFixture::decision(true);
    let id = proposal["id"].as_str().unwrap();
    assert!(matches!(
        f.work
            .store
            .decide_feature_map(&f.developer, &f.work.project.id, id, &decision),
        Err(PlatformError::Forbidden)
    ));
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.pmo.actor_id(),
            "PMO API",
            KeyScope {
                capabilities: MemberRole::Pmo.capabilities(),
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&key.key).unwrap();
    assert!(matches!(
        f.work
            .store
            .decide_feature_map(&api, &f.work.project.id, id, &decision),
        Err(PlatformError::Forbidden)
    ));
    assert!(
        !f.work
            .store
            .feature_catalog(&f.developer, &f.work.project.id)
            .unwrap()["can_decide"]
            .as_bool()
            .unwrap()
    );
    assert!(
        f.work
            .store
            .feature_catalog(&f.pmo, &f.work.project.id)
            .unwrap()["can_decide"]
            .as_bool()
            .unwrap()
    );
    let result = f
        .work
        .store
        .decide_feature_map(&f.pmo, &f.work.project.id, id, &decision)
        .unwrap();
    assert_eq!(result["status"], "approved");
    assert_eq!(f.counts(), (1, 1, 1, 1, 1));
    assert!(
        f.work
            .store
            .decide_feature_map(&f.pmo, &f.work.project.id, id, &decision)
            .is_err()
    );
    let feature = result["result"]["feature_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let catalog = f
        .work
        .store
        .feature_catalog(&f.developer, &f.work.project.id)
        .unwrap();
    assert_eq!(catalog["catalog_revision"], 1);
    assert_eq!(
        catalog["items"][0]["task_ids"][0],
        result["result"]["task_ids"][0]
    );
    let mut duplicate = f.proposal("  IDENTITY VERIFICATION  ", false);
    duplicate.expected_catalog_revision = 1;
    assert!(matches!(
        f.work
            .store
            .propose_feature_map(&f.pmo, &f.work.project.id, &duplicate),
        Err(PlatformError::Conflict(_))
    ));
    let second = f
        .work
        .store
        .create_project_document(
            &f.pmo,
            &f.work.project.id,
            &FeatureMapFixture::requirement_input("REQ-002", &f.requirement_flow),
        )
        .unwrap();
    duplicate.requirement_id = second.id;
    duplicate.requirement_version_id = second.latest_version_id;
    duplicate.links = vec![FeatureLinkProposal {
        feature_id: Some(feature.clone()),
        expected_revision: Some(1),
        name: None,
        description: None,
        impact: "Reuses shared identity capability".into(),
    }];
    let reuse = f
        .work
        .store
        .propose_feature_map(&f.pmo, &f.work.project.id, &duplicate)
        .unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.pmo.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    f.work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            reuse["id"].as_str().unwrap(),
            &decision,
        )
        .unwrap();
    assert_eq!(f.counts(), (1, 2, 1, 1, 1));
    let other = f
        .work
        .store
        .create_project(&f.work.ctx, "Outside PMO scope", None)
        .unwrap();
    assert!(
        f.work
            .store
            .decide_feature_map(&f.pmo, &other.id, id, &decision)
            .is_err()
    );
}
#[test]
#[ignore = "requires PostgreSQL; rejection, stale evidence and transactional rollback"]
fn rejected_stale_and_invalid_maps_never_partially_apply() {
    let mut f = FeatureMapFixture::new();
    let input = f.proposal("Rejected capability", true);
    let proposal = f
        .work
        .store
        .propose_feature_map(&f.pmo, &f.work.project.id, &input)
        .unwrap();
    let rejected = f
        .work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            proposal["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(false),
        )
        .unwrap();
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(f.counts(), (0, 0, 0, 0, 0));
    let mut invalid = f.proposal("Rolled back capability", true);
    let mut invalid_task = invalid.tasks[0].clone();
    invalid_task.definition.type_code = "missing_kind".into();
    invalid.tasks.push(invalid_task);
    let proposal = f
        .work
        .store
        .propose_feature_map(&f.pmo, &f.work.project.id, &invalid)
        .unwrap();
    assert!(
        f.work
            .store
            .decide_feature_map(
                &f.pmo,
                &f.work.project.id,
                proposal["id"].as_str().unwrap(),
                &FeatureMapFixture::decision(true)
            )
            .is_err()
    );
    assert_eq!(f.counts(), (0, 0, 0, 0, 0));
    let catalog = f
        .work
        .store
        .feature_catalog(&f.pmo, &f.work.project.id)
        .unwrap();
    assert_eq!(catalog["catalog_revision"], 0);
    assert!(
        catalog["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == proposal["id"] && p["status"] == "pending")
    );
    let stale = f
        .work
        .store
        .propose_feature_map(
            &f.pmo,
            &f.work.project.id,
            &f.proposal("Stale requirement", false),
        )
        .unwrap();
    let revision:ProjectDocumentRevise=serde_json::from_value(json!({"expected_revision":f.requirement.revision,"title":"Revised scope","content":FeatureMapFixture::requirement_content(),"origin":"declared_business","reason":"Changed requirements"})).unwrap();
    f.work
        .store
        .revise_project_document(&f.pmo, &f.work.project.id, &f.requirement.id, &revision)
        .unwrap();
    assert!(matches!(
        f.work.store.decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            stale["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true)
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(f.counts(), (0, 0, 0, 0, 0));
}
#[test]
#[ignore = "requires PostgreSQL; independent proposals survive catalog changes; duplicate capabilities remain blocked"]
fn unrelated_catalog_changes_preserve_pending_maps() {
    let mut f = FeatureMapFixture::new();
    let a = f
        .work
        .store
        .propose_feature_map(
            &f.pmo,
            &f.work.project.id,
            &f.proposal("First capability", false),
        )
        .unwrap();
    let b = f
        .work
        .store
        .propose_feature_map(
            &f.pmo,
            &f.work.project.id,
            &f.proposal("Second capability", false),
        )
        .unwrap();
    let stale = f
        .work
        .store
        .propose_feature_map(
            &f.pmo,
            &f.work.project.id,
            &f.proposal("Requirement will change", false),
        )
        .unwrap();
    let duplicate = f
        .work
        .store
        .propose_feature_map(
            &f.pmo,
            &f.work.project.id,
            &f.proposal("First capability", false),
        )
        .unwrap();
    f.work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            a["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        )
        .unwrap();
    f.work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            b["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        )
        .unwrap();
    assert!(matches!(
        f.work.store.decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            duplicate["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        ),
        Err(PlatformError::Conflict(
            "feature already exists; link it instead of creating it"
        ))
    ));
    let revision: ProjectDocumentRevise = serde_json::from_value(json!({
        "expected_revision":f.requirement.revision,"title":"Updated requirement",
        "content":FeatureMapFixture::requirement_content(),"origin":"declared_business",
        "reason":"Requirement changed after proposals"
    }))
    .unwrap();
    f.work
        .store
        .revise_project_document(&f.pmo, &f.work.project.id, &f.requirement.id, &revision)
        .unwrap();
    assert!(matches!(
        f.work.store.decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            stale["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        ),
        Err(PlatformError::Conflict(
            "requirement version changed; review the map again"
        ))
    ));
    assert_eq!(f.counts(), (2, 2, 0, 0, 0));
}

#[test]
#[ignore = "requires PostgreSQL; technical lead approval is human-only and project scoped"]
fn technical_lead_can_approve_features_but_its_agent_cannot() {
    let mut f = FeatureMapFixture::new();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.pmo.actor_id(),
            &MemberRoleChange {
                role: MemberRole::TechnicalLead,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    let proposal = f
        .work
        .store
        .propose_feature_map(
            &f.pmo,
            &f.work.project.id,
            &f.proposal("Lead reviewed feature", false),
        )
        .unwrap();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.pmo.actor_id(),
            "Lead automation",
            KeyScope {
                capabilities: MemberRole::TechnicalLead.capabilities(),
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&key.key).unwrap();
    let id = proposal["id"].as_str().unwrap();
    let decision = FeatureMapFixture::decision(true);
    assert!(matches!(
        f.work
            .store
            .decide_feature_map(&api, &f.work.project.id, id, &decision),
        Err(PlatformError::Forbidden)
    ));
    assert!(
        f.work
            .store
            .decide_feature_map(&f.pmo, &f.work.project.id, id, &decision)
            .is_ok()
    );
}

#[test]
#[ignore = "requires PostgreSQL; unassigned proposed tasks stay pending after human feature approval"]
fn feature_approval_creates_unassigned_pool_tasks_without_a_workflow() {
    let mut f = FeatureMapFixture::new();
    f.work
        .store
        .remove_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowRemoval {
                expected_revision: 1,
                reason: "Intake has no execution assignment".into(),
            },
        )
        .unwrap();
    let mut input = f.proposal("Unassigned intake", true);
    input.tasks[0].definition.assignee_id = None;
    input.tasks[0].definition.workflow_version_id = None;
    let proposal = f
        .work
        .store
        .propose_feature_map(&f.pmo, &f.work.project.id, &input)
        .unwrap();
    assert_eq!(f.counts(), (0, 0, 0, 0, 0));
    f.work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            proposal["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        )
        .unwrap();
    let page = f
        .work
        .store
        .task_page(&f.pmo, &f.work.project.id, &TaskListQuery::default())
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].lifecycle, TaskLifecycle::Pending);
    assert!(
        page.items[0].assignee_id.is_none()
            && page.items[0].workflow_version_id.is_none()
            && page.items[0].state_id.is_none()
    );
}
