#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
fn sql(f: &WorkItemFixture) -> postgres::Client {
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut client = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    client
}
fn developer(f: &mut WorkItemFixture, name: &str) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
    let actor = f.store.create_user(&f.ctx, name, &password, name).unwrap();
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
    let token = f.store.login(name, &password).unwrap();
    let verified = f
        .store
        .identity_gate(&token.access_token, "skip", "")
        .unwrap();
    f.store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap()
}
#[test]
#[ignore = "requires PostgreSQL; overtime requires own real task work and append-only verified push receipts inside the stored timezone interval"]
fn overtime_links_real_tasks_verified_actor_commits_and_preserves_approval() {
    let mut f = WorkItemFixture::new();
    let dev = developer(&mut f, "developer");
    let other = developer(&mut f, "other");
    let task = f.task("Planned overtime implementation");
    let wrong = f.task("Unassigned task");
    let mut db = sql(&f);
    db.execute(
        "UPDATE tasks SET assignee_id=$2 WHERE id=$1",
        &[&task.id, &dev.actor_id()],
    )
    .unwrap();
    let body = json!({"date":"2026-01-13","start_time":"18:00","end_time":"20:00","next_day":false,"reason":"Finish planned implementation","task_ids":[task.id]});
    let mut invalid = body.clone();
    invalid["task_ids"] = json!([wrong.id]);
    assert!(f.store.create_overtime_request(&dev, &invalid).is_err());
    invalid["task_ids"] = json!(["missing-task"]);
    assert!(f.store.create_overtime_request(&dev, &invalid).is_err());
    invalid["task_ids"] = json!([]);
    assert!(f.store.create_overtime_request(&dev, &invalid).is_err());
    let unrelated = f.store.create_project(&f.ctx, "Unrelated", None).unwrap();
    let role = f.store.member_role(&f.ctx, dev.actor_id()).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            dev.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![unrelated.id.clone()],
                expected_revision: role.revision,
            },
        )
        .unwrap();
    assert!(
        f.store.create_overtime_request(&dev, &body).is_err(),
        "a real own task in a revoked project is not eligible"
    );
    f.store
        .change_member_role(
            &f.ctx,
            dev.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: role.revision + 1,
            },
        )
        .unwrap();
    let request = f.store.create_overtime_request(&dev, &body).unwrap();
    let id = request["id"].as_str().unwrap();
    assert_eq!(request["status"], "pending");
    let planned = f.store.overtime_planned_evidence(&dev).unwrap();
    assert!(
        planned["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == task.id)
    );
    assert!(
        planned["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["id"] != wrong.id)
    );
    let repo = f
        .store
        .register_repository(
            &f.ctx,
            &f.project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/overtime".into(),
            },
        )
        .unwrap();
    let connection = SecretGenerator::id("gcn").unwrap();
    db.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','developer','active',1,'{}',now()+interval '1 hour')",&[&connection,&dev.actor_id()]).unwrap();
    db.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/overtime',connection_id=$1,authorization_status='authorized' WHERE id=$2",&[&connection,&repo.id]).unwrap();
    for (index, event, actor, hour) in [
        (1i64, "good", Some(dev.actor_id()), "18:30"),
        (2, "outside", Some(dev.actor_id()), "20:00"),
        (3, "unverified", None, "18:30"),
        (4, "other-sender", Some(other.actor_id()), "18:30"),
    ] {
        db.execute("INSERT INTO repository_source_events(id,repository_id,source_generation,provider,scope_id,delivery_id,source_ref,source_commit) VALUES($1,$2,$3,'gitlab','hook',$1,'refs/heads/main',$4)",&[&event,&repo.id,&index,&"a".repeat(40)]).unwrap();
        if let Some(actor) = actor {
            db.execute("INSERT INTO developer_push_activity(actor_id,provider,scope_id,delivery_id,project_id,occurred_at) VALUES($1,'gitlab','hook',$2,$3,('2026-01-13 '||$4::text||':00+00')::timestamptz)",&[&actor,&event,&f.project.id,&hour]).unwrap();
        }
    }
    let other_repo = f
        .store
        .register_repository(
            &f.ctx,
            &unrelated.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/unrelated".into(),
            },
        )
        .unwrap();
    db.execute("UPDATE project_repositories SET remote_id='102',remote_full_name='fixture/unrelated',connection_id=$1,authorization_status='authorized' WHERE id=$2",&[&connection,&other_repo.id]).unwrap();
    db.execute("INSERT INTO repository_source_events(id,repository_id,source_generation,provider,scope_id,delivery_id,source_ref,source_commit) VALUES('wrong-project',$1,1,'gitlab','hook','wrong-project','refs/heads/main',$2)",&[&other_repo.id,&"b".repeat(40)]).unwrap();
    db.execute("INSERT INTO developer_push_activity(actor_id,provider,scope_id,delivery_id,project_id,occurred_at) VALUES($1,'gitlab','hook','wrong-project',$2,'2026-01-13 18:30:00+00')",&[&dev.actor_id(),&unrelated.id]).unwrap();
    db.batch_execute("UPDATE labor_calendar_settings SET timezone='America/El_Salvador'")
        .unwrap();
    let candidates = f.store.overtime_evidence_candidates(&dev, id).unwrap();
    assert_eq!(candidates["commits"].as_array().unwrap().len(), 1);
    assert_eq!(candidates["commits"][0]["source_event_id"], "good");
    for event in [
        "outside",
        "unverified",
        "other-sender",
        "missing",
        "wrong-project",
    ] {
        assert!(
            f.store
                .append_overtime_evidence(&dev, id, &json!({"source_event_ids":[event]}))
                .is_err()
        );
    }
    assert!(
        f.store
            .append_overtime_evidence(&other, id, &json!({"source_event_ids":["good"]}))
            .is_err()
    );
    assert!(
        f.store
            .append_overtime_evidence(&dev, id, &json!({"sha":"a".repeat(40)}))
            .is_err()
    );
    f.store
        .decide_overtime_request(
            &f.ctx,
            id,
            &json!({"approved":true,"reason":"Authorized overtime"}),
        )
        .unwrap();
    let append = json!({"source_event_ids":["good"]});
    let evidence = f
        .store
        .request(
            &dev,
            "labor.overtime.evidence.append",
            "evidence-once",
            &append,
            |s| s.append_overtime_evidence(&dev, id, &append),
        )
        .unwrap();
    let replay = f
        .store
        .request(
            &dev,
            "labor.overtime.evidence.append",
            "evidence-once",
            &append,
            |s| s.append_overtime_evidence(&dev, id, &append),
        )
        .unwrap();
    assert_eq!(
        evidence, replay,
        "idempotent replays preserve the response shape and evidence"
    );
    let changed = json!({"source_event_ids":["good"],"task_ids":[task.id]});
    assert!(matches!(
        f.store.request(
            &dev,
            "labor.overtime.evidence.append",
            "evidence-once",
            &changed,
            |s| s.append_overtime_evidence(&dev, id, &changed)
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        evidence["commits"][0]["provenance"],
        "verified_push_receipt"
    );
    f.store
        .append_overtime_evidence(&dev, id, &json!({"source_event_ids":["good"]}))
        .unwrap();
    let row=db.query_one("SELECT status,minutes,(SELECT count(*) FROM overtime_commit_links WHERE request_id=$1) FROM labor_overtime_requests WHERE id=$1",&[&id]).unwrap();
    assert_eq!(row.get::<_, String>(0), "approved");
    assert_eq!(row.get::<_, i32>(1), 120);
    assert_eq!(row.get::<_, i64>(2), 1);
    assert!(
        db.execute(
            "DELETE FROM overtime_commit_links WHERE request_id=$1",
            &[&id]
        )
        .is_err()
    );
    let visible = f.store.overtime(&dev).unwrap();
    assert_eq!(visible["items"][0]["tasks"][0]["id"], task.id);
    assert_eq!(visible["items"][0]["commits"].as_array().unwrap().len(), 1);
    // Revoking current business scope hides names and disables append while retaining immutable IDs.
    let role = f.store.member_role(&f.ctx, dev.actor_id()).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            dev.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![unrelated.id],
                expected_revision: role.revision,
            },
        )
        .unwrap();
    let hidden = f.store.overtime(&dev).unwrap();
    assert!(hidden["items"][0]["tasks"].as_array().unwrap().is_empty());
    assert!(hidden["items"][0]["commits"].as_array().unwrap().is_empty());
    assert_eq!(hidden["items"][0]["can_attach_evidence"], false);
    assert_eq!(hidden["items"][0]["evidence_unavailable"], true);
    assert_eq!(hidden["items"][0]["evidence_unavailable_count"], 2);
    assert!(
        f.store
            .append_overtime_evidence(&dev, id, &json!({"source_event_ids":["good"]}))
            .is_err()
    );
}
