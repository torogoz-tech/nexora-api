#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::*;

#[test]
#[ignore = "requires PostgreSQL; project profile authorization and real activity"]
fn project_profile_uses_live_scoped_authority_and_visible_records() {
    let mut f = FeatureMapFixture::new();
    let project = f.work.project.id.clone();
    let other = f
        .work
        .store
        .create_project(&f.work.ctx, "Private profile", None)
        .unwrap();
    let _task = f
        .work
        .store
        .create_task(
            &f.developer,
            &project,
            &TaskCreate {
                title: "Visible priority task".into(),
                type_code: "task".into(),
                assignee_id: Some(f.developer.actor_id().into()),
                priority: TaskPriority::High,
                origin: "Profile fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let unrelated = f
        .work
        .store
        .create_task(
            &f.work.ctx,
            &project,
            &TaskCreate {
                title: "Unrelated private task".into(),
                type_code: "task".into(),
                assignee_id: Some(f.developer.actor_id().into()),
                priority: TaskPriority::Critical,
                origin: "Profile fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    f.sql.execute("INSERT INTO developer_workflows(actor_id,workflow_id,workflow_version_id,assigned_by) SELECT $1,workflow_id,workflow_version_id,$1 FROM developer_workflows WHERE actor_id=$2", &[&f.work.ctx.actor_id(),&f.developer.actor_id()]).unwrap();
    f.sql
        .execute(
            "UPDATE tasks SET assignee_id=$2 WHERE id=$1",
            &[&unrelated.id, &f.work.ctx.actor_id()],
        )
        .unwrap();
    f.sql.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name,authorization_status) VALUES('repo_profile_local',$1,'local_snapshot','Authorized local','authorized'),('repo_profile_revoked',$1,'local_snapshot','Revoked local','revoked')", &[&project]).unwrap();
    for index in 0..17 {
        f.work
            .store
            .create_task(
                &f.developer,
                &project,
                &TaskCreate {
                    title: format!("Additional visible work {index}"),
                    type_code: "task".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    priority: TaskPriority::High,
                    origin: "Profile fixture".into(),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&f.pmo.actor_id()],
        )
        .unwrap();
    let profile = f
        .work
        .store
        .project_profile(&f.developer, &project, None)
        .unwrap();
    assert_eq!(profile["project"]["id"], project);
    assert_eq!(profile["counts"]["tasks"], 18);
    assert_eq!(profile["counts"]["requirements"], 1);
    assert_eq!(profile["counts"]["documents"], 0);
    assert_eq!(profile["priority_tasks"].as_array().unwrap().len(), 8);
    assert_eq!(profile["activity"].as_array().unwrap().len(), 19);
    assert!(
        profile["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["id"] != f.pmo.actor_id())
    );
    assert_eq!(profile["repositories"].as_array().unwrap().len(), 1);
    assert!(
        profile["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member.get("email").is_none() && member.get("has_password").is_none())
    );
    assert!(
        profile["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["title"] != unrelated.title)
    );
    let expected: i64 = f.sql.query_one("SELECT (SELECT count(*) FROM task_history WHERE project_id=$1 AND task_id<>$2)+(SELECT count(*) FROM project_document_versions WHERE project_id=$1)", &[&project,&unrelated.id]).unwrap().get(0);
    assert_eq!(
        profile["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|day| day["count"].as_i64().unwrap())
            .sum::<i64>(),
        expected
    );
    assert!(matches!(
        f.work.store.project_profile(&f.developer, &other.id, None),
        Err(PlatformError::Forbidden)
    ));
    let historical = f
        .work
        .store
        .project_profile(&f.developer, &project, Some(2000))
        .unwrap();
    assert!(historical["contributions"].as_array().unwrap().is_empty());
    assert!(matches!(
        f.work
            .store
            .project_profile(&f.developer, &project, Some(1999)),
        Err(PlatformError::Validation(_))
    ));
    let restricted = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Project only profile",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let restricted_ctx = f.work.store.authenticate(&restricted.key).unwrap();
    let restricted_profile = f
        .work
        .store
        .project_profile(&restricted_ctx, &project, None)
        .unwrap();
    assert_eq!(restricted_profile["counts"]["tasks"], 0);
    assert_eq!(restricted_profile["counts"]["requirements"], 0);
    assert!(
        restricted_profile["activity"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        restricted_profile["repositories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    f.sql
        .execute("UPDATE projects SET archived=true WHERE id=$1", &[&project])
        .unwrap();
    let archived = f
        .work
        .store
        .project_profile(&f.developer, &project, None)
        .unwrap();
    assert_eq!(archived["project"]["archived"], true);
    assert_eq!(archived["counts"]["tasks"], 0);
    assert!(archived["activity"].as_array().unwrap().is_empty());
    f.sql
        .execute(
            "DELETE FROM actor_grants WHERE actor_id=$1 AND capability='projects:read'",
            &[&f.developer.actor_id()],
        )
        .unwrap();
    assert!(matches!(
        f.work.store.project_profile(&f.developer, &project, None),
        Err(PlatformError::Forbidden)
    ));
}
