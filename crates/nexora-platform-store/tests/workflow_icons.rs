#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; business area icons persist alongside legacy selections"]
fn workflow_business_area_icons_are_supported_by_the_database() {
    let mut f = WorkItemFixture::new();
    for icon in [
        WorkflowIcon::Palette,
        WorkflowIcon::Infinity,
        WorkflowIcon::ClipboardCheck,
        WorkflowIcon::Compass,
        WorkflowIcon::AppWindow,
        WorkflowIcon::Server,
        WorkflowIcon::Smartphone,
        WorkflowIcon::Database,
        WorkflowIcon::Headset,
        WorkflowIcon::SwatchBook,
        WorkflowIcon::PenTool,
        WorkflowIcon::MousePointer,
        WorkflowIcon::Accessibility,
        WorkflowIcon::Monitor,
        WorkflowIcon::Component,
        WorkflowIcon::Terminal,
        WorkflowIcon::Cloud,
        WorkflowIcon::Container,
        WorkflowIcon::Network,
        WorkflowIcon::Cpu,
        WorkflowIcon::Blocks,
        WorkflowIcon::TestTube,
        WorkflowIcon::CircleCheck,
        WorkflowIcon::Gauge,
        WorkflowIcon::ScanSearch,
        WorkflowIcon::LockKeyhole,
        WorkflowIcon::Target,
        WorkflowIcon::Presentation,
        WorkflowIcon::CalendarDays,
        WorkflowIcon::Users,
        WorkflowIcon::Handshake,
        WorkflowIcon::Megaphone,
        WorkflowIcon::ShoppingCart,
        WorkflowIcon::GraduationCap,
        WorkflowIcon::HeartPulse,
        WorkflowIcon::Banknote,
    ] {
        let input: DeveloperWorkflowCreate = serde_json::from_value(json!({"name":format!("Area {}",icon.as_str()),"icon":icon,"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap();
        let created = f.store.create_developer_workflow(&f.ctx, &input).unwrap();
        assert_eq!(
            f.store
                .developer_workflow(&f.ctx, &created.id)
                .unwrap()
                .icon,
            icon
        );
    }
}
#[test]
#[ignore = "requires PostgreSQL; durable current metadata with immutable published versions and exact retries"]
fn workflow_icons_persist_in_current_metadata_without_changing_publication() {
    let mut f = WorkItemFixture::new();
    let input:DeveloperWorkflowCreate=serde_json::from_value(json!({"name":"Delivery icon","icon":"rocket","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap();
    let body = json!(input);
    let created = f
        .store
        .request(&f.ctx, "workflow.create", "icon-create", &body, |s| {
            s.create_developer_workflow(&f.ctx, &input)
        })
        .unwrap();
    let replay = f
        .store
        .request(&f.ctx, "workflow.create", "icon-create", &body, |s| {
            s.create_developer_workflow(&f.ctx, &input)
        })
        .unwrap();
    assert_eq!(created.id, replay.id);
    assert_eq!(created.icon, WorkflowIcon::Rocket);
    let published = f
        .store
        .publish_developer_workflow(
            &f.ctx,
            &created.id,
            &WorkflowPublish {
                expected_revision: created.revision,
                reason: "Publish graph".into(),
            },
        )
        .unwrap();
    let current = f.store.developer_workflow(&f.ctx, &created.id).unwrap();
    let original_graph = serde_json::to_value(&current.draft).unwrap();
    let original_version = serde_json::to_value(&published).unwrap();
    let edit = DeveloperWorkflowEdit {
        expected_revision: current.revision,
        icon: Some(WorkflowIcon::ShieldCheck),
        reason: "Change display icon".into(),
        ..Default::default()
    };
    let body = json!({"workflow_id":created.id,"change":edit});
    let changed = f
        .store
        .request(&f.ctx, "workflow.change", "icon-edit", &body, |s| {
            s.change_developer_workflow(&f.ctx, &created.id, &edit)
        })
        .unwrap();
    let replay = f
        .store
        .request(&f.ctx, "workflow.change", "icon-edit", &body, |s| {
            s.change_developer_workflow(&f.ctx, &created.id, &edit)
        })
        .unwrap();
    assert_eq!(changed.revision, replay.revision);
    assert_eq!(changed.revision, current.revision + 1);
    assert_eq!(changed.icon, WorkflowIcon::ShieldCheck);
    assert_eq!(changed.latest_version_id, current.latest_version_id);
    assert_eq!(changed.latest_version_number, 1);
    assert_eq!(
        serde_json::to_value(&changed.draft).unwrap(),
        original_graph
    );
    assert!(
        f.store
            .change_developer_workflow(&f.ctx, &created.id, &edit)
            .is_err()
    );
    let unchanged = f
        .store
        .developer_workflow_version(&f.ctx, &created.id, &published.id)
        .unwrap();
    assert_eq!(serde_json::to_value(&unchanged).unwrap(), original_version);
    assert!(original_version.get("icon").is_none());
    assert_eq!(
        f.store
            .developer_workflow_versions(&f.ctx, &created.id, None, 10)
            .unwrap()
            .items
            .len(),
        1
    );
    let listed = f
        .store
        .developer_workflow_page(&f.ctx, None, 100, false)
        .unwrap();
    assert_eq!(
        listed
            .items
            .iter()
            .find(|w| w.id == created.id)
            .unwrap()
            .icon,
        WorkflowIcon::ShieldCheck
    );
    let mut reloaded = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        reloaded
            .developer_workflow(&f.ctx, &created.id)
            .unwrap()
            .icon,
        WorkflowIcon::ShieldCheck
    );
    let omission = DeveloperWorkflowEdit {
        expected_revision: changed.revision,
        purpose: Some("Keep icon".into()),
        reason: "Purpose metadata".into(),
        ..Default::default()
    };
    let retained = f
        .store
        .change_developer_workflow(&f.ctx, &created.id, &omission)
        .unwrap();
    assert_eq!(retained.icon, WorkflowIcon::ShieldCheck);
    let legacy: WorkflowCreate = serde_json::from_value(
        json!({"name":"Legacy defaults","resource_kinds":["task"],"graph":input.graph}),
    )
    .unwrap();
    let legacy = f
        .store
        .create_workflow(&f.ctx, &f.project.id, &legacy)
        .unwrap();
    assert_eq!(legacy.icon, WorkflowIcon::Workflow);
    let change = WorkflowChange {
        expected_revision: legacy.revision,
        icon: Some(WorkflowIcon::Bug),
        reason: "Project icon".into(),
        ..Default::default()
    };
    assert_eq!(
        f.store
            .change_workflow(&f.ctx, &f.project.id, &legacy.id, &change)
            .unwrap()
            .icon,
        WorkflowIcon::Bug
    );
    let password = zeroize::Zeroizing::new(SecretGenerator::token("icon_auth").unwrap());
    let actor = f
        .store
        .create_user(&f.ctx, "viewer", &password, "Viewer")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login("viewer", &password).unwrap();
    let viewer = f.store.authenticate(&session.access_token).unwrap();
    assert!(f.store.create_developer_workflow(&viewer, &input).is_err());
    let forbidden = DeveloperWorkflowEdit {
        expected_revision: retained.revision,
        icon: Some(WorkflowIcon::Code),
        reason: "Forbidden icon change".into(),
        ..Default::default()
    };
    assert!(
        f.store
            .change_developer_workflow(&viewer, &created.id, &forbidden)
            .is_err()
    );
}
