#[allow(clippy::duplicate_mod)]
#[path = "../tests/support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[allow(clippy::duplicate_mod)]
#[path = "../tests/support/task_test_sql.rs"]
mod test_sql;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL; stale worker observation must not rewrite completed jobs"]
fn a_stale_queued_observation_is_reloaded_after_lock_and_preserves_success() {
    let mut f = TaskPoolFixture::new(false);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.execute("INSERT INTO task_releases(id,project_id,version,branch_name,created_by,revision) VALUES('release-stale',$1,'stale','release/stale',$2,2)",&[&f.work.project.id,&f.work.ctx.actor_id()]).unwrap();
    sql.execute("INSERT INTO task_release_composition_jobs(id,release_id,project_id,release_revision,actor_id,credential_id,auth_method,status,record) VALUES('job-stale','release-stale',$1,1,$2,$3,'session','queued','{}')",&[&f.work.project.id,&f.work.ctx.actor_id(),&f.work.ctx.credential_id()]).unwrap();
    let stale = sql
        .query_one(
            "SELECT id,release_id FROM task_release_composition_jobs WHERE status='queued'",
            &[],
        )
        .unwrap();
    let job: String = stale.get(0);
    let release: String = stale.get(1);
    sql.execute("UPDATE task_release_composition_jobs SET status='succeeded',record='{\"confirmed\":true}' WHERE id=$1",&[&job]).unwrap();
    let worked = f
        .work
        .store
        .compose_release_candidate(
            &crate::GitNetworkPolicy::default(),
            std::path::Path::new("/usr/bin/git"),
            std::path::Path::new("/tmp"),
            &job,
            &release,
        )
        .unwrap();
    assert!(!worked);
    let row = sql
        .query_one(
            "SELECT status,record FROM task_release_composition_jobs WHERE id=$1",
            &[&job],
        )
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "succeeded");
    assert_eq!(
        row.get::<_, serde_json::Value>(1),
        serde_json::json!({"confirmed":true})
    );
}
