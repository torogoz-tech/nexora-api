#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn releases_pin_explicit_task_revisions_and_source_commits_without_completing_or_publishing() {
    let mut f = TaskPoolFixture::new(false);
    let project = f.work.project.id.clone();
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "release_work".into(),
                name: "Release work".into(),
                purpose: TaskPurpose::Implementation,
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(project.clone()),
                definition: TaskCreate {
                    title: "Selected task".into(),
                    type_code: "release_work".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    entry_workflow_id: Some(f.version.workflow_id.clone()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    let active = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: Some(f.version.workflow_id.clone()),
            },
            Some(&project),
        )
        .unwrap();
    let ready = f
        .work
        .store
        .transition_pool_task(
            &f.developer,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: active.revision,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ready.lifecycle, TaskLifecycle::Active);
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES('release-connection','gitlab','fixture.local',$1,'1','fixture','active',1,'{}',now()+interval '1 hour')",&[&f.work.ctx.actor_id()]).unwrap();
    sql.execute("INSERT INTO project_repositories(id,project_id,provider,host,requested_full_name,remote_id,remote_full_name,connection_id,authorization_status) VALUES('repo_release',$1,'gitlab','fixture.local','fixture/release','1','fixture/release','release-connection','authorized')",&[&project]).unwrap();
    let task_head = "1".repeat(40);
    let release_head = "2".repeat(40);
    sql.execute("INSERT INTO task_repository_branches(task_id,repository_id,branch_name,base_branch,base_sha,head_sha,evidence) VALUES($1,'repo_release','feature/selected','develop',$2,$2,'{\"provenance\":\"provider_verified\"}')",&[&task.id,&task_head]).unwrap();
    sql.execute("INSERT INTO task_commit_links(id,task_id,repository_id,commit_sha,sequence,provider_evidence,actor_id) VALUES('explicit-selected-commit',$1,'repo_release',$2,1,'{\"provenance\":\"provider_verified\"}',$3)",&[&task.id,&task_head,&f.developer.actor_id()]).unwrap();
    let input = json!({"version":"1.0.0","branch_name":"release/1.0.0","tasks":[{"task_id":task.id,"expected_revision":ready.revision}],"repositories":[{"repository_id":"repo_release","head_sha":release_head}]});
    assert!(matches!(
        f.work
            .store
            .save_task_release(&f.developer, &project, None, &input),
        Err(PlatformError::Forbidden)
    ));
    let release = f
        .work
        .store
        .save_task_release(&f.manager, &project, None, &input)
        .unwrap();
    assert_eq!(release["status"], "prepared");
    assert_eq!(release["tasks"][0]["task_revision"], ready.revision);
    assert_eq!(release["sources"][0]["source_head_sha"], task_head);
    assert_eq!(release["repositories"][0]["head_sha"], release_head);
    assert_eq!(
        f.work
            .store
            .pool_task(&f.developer, &task.id)
            .unwrap()
            .lifecycle,
        TaskLifecycle::Active
    );
    let publications: i64 = sql
        .query_one(
            "SELECT count(*) FROM task_production_publications WHERE task_id=$1",
            &[&task.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(publications, 0);
    let release_id = release["id"].as_str().unwrap();
    assert_eq!(
        f.work
            .store
            .task_release(&f.manager, &project, release_id)
            .unwrap()["sources"],
        release["sources"]
    );
    let mut assertion = input.clone();
    assertion["status"] = json!("published");
    assert!(matches!(
        f.work
            .store
            .save_task_release(&f.manager, &project, None, &assertion),
        Err(PlatformError::Validation(_))
    ));
    let added =
        json!({"expected_revision":0,"tasks":input["tasks"],"repositories":input["repositories"]});
    assert!(matches!(
        f.work
            .store
            .save_task_release(&f.manager, &project, Some(release_id), &added),
        Err(PlatformError::Conflict(_))
    ));
    let queued = f
        .work
        .store
        .queue_release_composition(&f.manager, &project, release_id, 1)
        .unwrap();
    assert_eq!(queued["status"], "queued");
    let again = f
        .work
        .store
        .queue_release_composition(&f.manager, &project, release_id, 1)
        .unwrap();
    assert_eq!(again["id"], queued["id"]);
    sql.execute(
        "UPDATE sessions SET revoked_at=clock_timestamp() WHERE id=$1",
        &[&f.manager.credential_id()],
    )
    .unwrap();
    assert!(
        f.work
            .store
            .compose_one_release(
                &nexora_platform_store::GitNetworkPolicy::default(),
                std::path::Path::new("/usr/bin/git"),
                std::path::Path::new("/tmp")
            )
            .unwrap()
    );
    let status: String = sql
        .query_one(
            "SELECT status FROM task_release_composition_jobs WHERE id=$1",
            &[&queued["id"].as_str().unwrap()],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "failed");
}
