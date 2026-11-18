#[allow(clippy::duplicate_mod)]
#[path = "../tests/support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[allow(clippy::duplicate_mod)]
#[path = "../tests/support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL; provider verification is supplied only to private writer"]
fn removing_and_rebinding_same_commit_restores_identity_with_fresh_evidence_revision_and_history() {
    let mut f = TaskPoolFixture::new(false);
    let project = f.work.project.id.clone();
    let task = f.create(Some(f.developer.actor_id().into()), Some(project.clone()));
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES('binding-connection','gitlab','fixture.local',$1,'1','fixture','active',1,'{}',now()+interval '1 hour')",&[&f.work.ctx.actor_id()]).unwrap();
    sql.execute("INSERT INTO project_repositories(id,project_id,provider,host,requested_full_name,remote_id,remote_full_name,connection_id,authorization_status) VALUES('repo_binding',$1,'gitlab','fixture.local','fixture/binding','1','fixture/binding','binding-connection','authorized')",&[&project]).unwrap();
    sql.execute("INSERT INTO git_connection_projects(connection_id,project_id,granted_by) VALUES('binding-connection',$1,$2)",&[&project,&f.work.ctx.actor_id()]).unwrap();
    let head = "2".repeat(40);
    let authority = {
        let mut tx = f.work.store.client.transaction().unwrap();
        let authority =
            crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                &mut tx,
                &f.developer,
                &project,
                "repo_binding",
            )
            .unwrap();
        tx.commit().unwrap();
        authority
    };
    let mut evidence = json!({"provenance":"provider_verified","authority":authority,"verification_nonce":"first"});
    let link = f
        .work
        .store
        .write_verified_task_commit(
            &f.developer,
            &project,
            &task.id,
            task.revision,
            "repo_binding",
            &head,
            &evidence,
        )
        .unwrap();
    let id = link["id"].as_str().unwrap();
    let current = f.work.store.pool_task(&f.developer, &task.id).unwrap();
    assert_eq!(current.revision, task.revision + 1);
    f.work
        .store
        .remove_task_commit(&f.developer, &task.id, id, current.revision)
        .unwrap();
    assert!(
        f.work.store.task_commits(&f.developer, &task.id).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let current = f.work.store.pool_task(&f.developer, &task.id).unwrap();
    evidence["verification_nonce"] = json!("fresh");
    assert!(matches!(
        f.work.store.write_verified_task_commit(
            &f.developer,
            &project,
            &task.id,
            current.revision - 1,
            "repo_binding",
            &head,
            &evidence
        ),
        Err(PlatformError::Conflict(_))
    ));
    let restored = f
        .work
        .store
        .write_verified_task_commit(
            &f.developer,
            &project,
            &task.id,
            current.revision,
            "repo_binding",
            &head,
            &evidence,
        )
        .unwrap();
    assert_eq!(restored["id"], id);
    assert_eq!(
        f.work
            .store
            .pool_task(&f.developer, &task.id)
            .unwrap()
            .revision,
        task.revision + 3
    );
    let row=sql.query_one("SELECT removed_at IS NULL,provider_evidence->>'verification_nonce' FROM task_commit_links WHERE id=$1",&[&id]).unwrap();
    assert!(row.get::<_, bool>(0));
    assert_eq!(row.get::<_, String>(1), "fresh");
    let count:i64=sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation IN ('task.commit_bound','task.commit_removed')",&[&task.id]).unwrap().get(0);
    assert_eq!(count, 3);
}
