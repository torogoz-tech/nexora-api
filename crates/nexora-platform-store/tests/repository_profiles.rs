#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::*;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL; explicit repository attribution and per-task visibility"]
fn repository_profile_activity_uses_explicit_links_and_current_task_scope() {
    let mut f = FeatureMapFixture::new();
    let project = f.work.project.id.clone();
    let owner = f.work.ctx.clone();
    let repository = f
        .work
        .store
        .register_repository(
            &owner,
            &project,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/profile".into(),
            },
        )
        .unwrap();
    f.sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES('conn_profile','gitlab','gitlab.com',$1,'42','fixture','active',1,'{}',now()+interval '1 hour')", &[&owner.actor_id()]).unwrap();
    f.sql.execute("INSERT INTO git_connection_projects(connection_id,project_id,granted_by) VALUES('conn_profile',$1,$2)", &[&project,&owner.actor_id()]).unwrap();
    f.sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name=requested_full_name,connection_id='conn_profile',authorization_status='authorized',revision=2 WHERE id=$1", &[&repository.id]).unwrap();
    let linked = f.work.task("Visible explicitly linked task");
    let hidden = f.work.task("Other developer linked task");
    let unrelated = f.work.task("Unrelated project task");
    let branch = f.work.task("Explicit work session task");
    f.sql
        .execute(
            "UPDATE tasks SET assignee_id=$1,priority='critical' WHERE id=ANY($2)",
            &[
                &f.developer.actor_id(),
                &vec![linked.id.clone(), unrelated.id.clone(), branch.id.clone()],
            ],
        )
        .unwrap();
    for (id, task) in [
        ("pr_profile_visible", &linked.id),
        ("pr_profile_hidden", &hidden.id),
    ] {
        f.sql.execute("INSERT INTO pull_request_links(id,project_id,repository_id,remote_id,pull_request_number,kind,task_id,created_by) VALUES($1,$2,$3,'101',7,'task',$4,$5)", &[&id,&project,&repository.id,&task,&owner.actor_id()]).unwrap();
    }
    f.sql.execute("INSERT INTO task_work_sessions(id,project_id,task_id,actor_id,task_revision,workflow_version_id,transition_id,record) SELECT 'work_profile',$1,t.id,$2,t.revision,t.workflow_version_id,'finish',$3 FROM tasks t WHERE t.id=$4", &[&project,&owner.actor_id(),&json!({"branches":[{"repository_id":repository.id}]}),&branch.id]).unwrap();
    let all = f
        .work
        .store
        .repository_profile(&owner, &project, &repository.id, None)
        .unwrap();
    assert_eq!(all["counts"]["tasks"], 3);
    assert_eq!(all["activity"].as_array().unwrap().len(), 3);
    assert_eq!(
        all["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|day| day["count"].as_i64().unwrap())
            .sum::<i64>(),
        3
    );
    assert!(!all.to_string().contains("Unrelated project task"));
    let own = f
        .work
        .store
        .repository_profile(&f.developer, &project, &repository.id, None)
        .unwrap();
    assert_eq!(own["counts"]["tasks"], 2);
    assert_eq!(own["priority_tasks"].as_array().unwrap().len(), 2);
    assert_eq!(own["activity"].as_array().unwrap().len(), 2);
    assert!(!own.to_string().contains("Other developer linked task"));
    assert!(!own.to_string().contains("Unrelated project task"));
    assert!(
        own["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["kind"] == "task")
    );
    // Repository activity includes commits even before an author alias is configured.
    f.sql.execute("INSERT INTO git_commit_authorship(id,provider,host,remote_repository_id,repository_id,project_id,commit_sha,author_name,scope_id,delivery_id) VALUES('commit_profile','gitlab','gitlab.com','101',$1,$2,'abc123','Unmatched author','scope','delivery')", &[&repository.id,&project]).unwrap();
    let with_commit = f
        .work
        .store
        .repository_profile(&f.developer, &project, &repository.id, None)
        .unwrap();
    assert!(
        with_commit["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "commit" && event["title"] == "abc123")
    );
    f.sql
        .execute(
            "DELETE FROM git_commit_authorship WHERE id='commit_profile'",
            &[],
        )
        .unwrap();
    f.sql.execute("INSERT INTO pull_request_links(id,project_id,repository_id,remote_id,pull_request_number,kind,document_id,version_id,created_by) VALUES('pr_profile_document',$1,$2,'101',7,'requirement',$3,$4,$5)", &[&project,&repository.id,&f.requirement.id,&f.requirement.latest_version_id,&owner.actor_id()]).unwrap();
    let documents = f
        .work
        .store
        .repository_profile(&f.developer, &project, &repository.id, None)
        .unwrap();
    assert_eq!(
        documents["activity"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "document")
            .count(),
        1
    );
    f.sql
        .execute(
            "UPDATE project_documents SET archived=true WHERE id=$1",
            &[&f.requirement.id],
        )
        .unwrap();
    let archived = f
        .work
        .store
        .repository_profile(&f.developer, &project, &repository.id, None)
        .unwrap();
    assert!(
        archived["activity"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["kind"] == "task")
    );
    let key = f
        .work
        .store
        .issue_key(
            &owner,
            f.developer.actor_id(),
            "Repository metadata only",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let attenuated = f.work.store.authenticate(&key.key).unwrap();
    let restricted = f
        .work
        .store
        .repository_profile(&attenuated, &project, &repository.id, None)
        .unwrap();
    assert_eq!(restricted["counts"]["tasks"], 0);
    assert_eq!(restricted["activity"], json!([]));
    f.sql.execute("UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id='conn_profile'", &[]).unwrap();
    assert!(matches!(
        f.work
            .store
            .repository_profile(&owner, &project, &repository.id, None),
        Err(PlatformError::NotFound)
    ));
}
