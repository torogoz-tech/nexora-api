#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; concurrent decisions, request immutability and atomic single authorization"]
fn approval_requests_retain_operations_and_concurrent_decisions_grant_only_once() {
    let mut f = WorkItemFixture::new();
    f.task("Keep ordinary flow fixture active");
    let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Concurrent approval","purpose":"One human decision","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Publish approval policy".into(),
            },
        )
        .unwrap();
    let task=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Approve once","type_code":"task","workflow_version_id":flow.id,"origin":"Concurrent fixture"})).unwrap()).unwrap();
    let request=f.store.create_approval_request(&f.ctx,&f.project.id,&serde_json::from_value(json!({"target":{"kind":"task","task_id":task.id,"transition_id":"finish","parameters":{"expected_revision":task.revision,"reason":"Execute reviewed operation"}},"reason":"Concurrent human review","expires_in_seconds":3600})).unwrap()).unwrap();
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host("127.0.0.1")
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    for statement in [
        "UPDATE approval_requests SET transition_id='other' WHERE id=$1",
        "UPDATE approval_requests SET expires_at=expires_at+interval '1 day' WHERE id=$1",
        "DELETE FROM approval_requests WHERE id=$1",
        "UPDATE approval_requests SET status='approved',revision=revision+1 WHERE id=$1",
    ] {
        assert_eq!(
            sql.execute(statement, &[&request.id]).unwrap_err().code(),
            Some(&postgres::error::SqlState::CHECK_VIOLATION)
        );
    }
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..2 {
            let config = &f.config;
            let namespace = &f.namespace;
            let ctx = &f.ctx;
            let project = &f.project.id;
            let id = &request.id;
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                let mut store = PlatformStore::connect(config, namespace).unwrap();
                barrier.wait();
                store.decide_approval_request(
                    ctx,
                    project,
                    id,
                    &ApprovalRequestDecide {
                        expected_revision: 1,
                        decision: ApprovalDecision::Approved,
                        reason: "Human decision".into(),
                        approval_expires_in_seconds: 600,
                    },
                )
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    let count: i64 = sql
        .query_one(
            "SELECT count(*) FROM task_approvals WHERE task_id=$1",
            &[&task.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    let current = f
        .store
        .get_approval_request(&f.ctx, &f.project.id, &request.id)
        .unwrap();
    assert_eq!(current.status, ApprovalRequestStatus::Approved);
    assert_eq!(current.revision, 2);
    assert_eq!(sql.execute("UPDATE approval_requests SET status='rejected',task_approval_id=NULL,revision=revision+1 WHERE id=$1",&[&request.id]).unwrap_err().code(),Some(&postgres::error::SqlState::CHECK_VIOLATION));
    let count:i64=sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation='approval_request.decided'",&[&task.id]).unwrap().get(0);
    assert_eq!(count, 1);
}
