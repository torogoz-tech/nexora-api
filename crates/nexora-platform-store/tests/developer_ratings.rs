#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::{Value, json};
use work_item_fixture::WorkItemFixture;

fn connection(f: &WorkItemFixture) -> postgres::Client {
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    sql
}
fn member(
    f: &mut WorkItemFixture,
    name: &str,
    role: MemberRole,
    projects: Vec<String>,
) -> AuthContext {
    let password =
        zeroize::Zeroizing::new(nexora_platform_store::SecretGenerator::token("password").unwrap());
    let actor = f.store.create_user(&f.ctx, name, &password, name).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role,
                all_projects: false,
                project_ids: projects,
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login(name, &password).unwrap();
    let verified = f
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    f.store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap()
}
fn day(report: &Value, date: &str) -> Value {
    report["daily"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["date"] == date)
        .unwrap()
        .clone()
}

#[test]
#[ignore = "requires PostgreSQL; scoped human ratings, stale roles, and self-rating prohibition"]
fn ratings_require_current_shared_project_developer_and_human_session() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let target = member(
        &mut f,
        "rated",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let peer = member(&mut f, "peer", MemberRole::Developer, vec![project.clone()]);
    let manager = member(
        &mut f,
        "manager",
        MemberRole::Manager,
        vec![project.clone()],
    );
    let other = f.store.create_project(&f.ctx, "Unrelated", None).unwrap();
    let outsider = member(&mut f, "outsider", MemberRole::Developer, vec![other.id]);
    let mut sql = connection(&f);
    sql.batch_execute("UPDATE labor_calendar_settings SET starts_on='2026-01-01',workdays=ARRAY[1,2,3,4,5,6,7]; UPDATE actors SET created_at='2026-01-01'; UPDATE actor_roles SET rating_started_at='2026-01-01'").unwrap();
    let input = json!({"date":"2026-01-02","project_id":project,"score":4.0,"reason":"Reviewed implementation","evidence_url":"https://example.com/review"});
    assert!(
        f.store
            .create_member_rating(&target, target.actor_id(), &input)
            .is_err()
    );
    assert!(
        f.store
            .create_member_rating(&outsider, target.actor_id(), &input)
            .is_err()
    );
    assert_eq!(
        f.store
            .create_member_rating(&peer, target.actor_id(), &input)
            .unwrap()["source"],
        "peer"
    );
    assert_eq!(
        f.store
            .create_member_rating(&manager, target.actor_id(), &input)
            .unwrap()["source"],
        "manager"
    );
    assert!(
        f.store
            .create_member_rating(&manager, manager.actor_id(), &input)
            .is_err()
    );
    let mut forged = input.clone();
    forged["source"] = json!("ai");
    assert!(
        f.store
            .create_member_rating(&peer, target.actor_id(), &forged)
            .is_err()
    );
    let key = f
        .store
        .issue_key(
            &f.ctx,
            manager.actor_id(),
            "No key ratings",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&key.key).unwrap();
    assert!(
        f.store
            .create_member_rating(&api, target.actor_id(), &input)
            .is_err()
    );
    let hidden = f
        .store
        .create_project(&f.ctx, "Private review", None)
        .unwrap();
    sql.execute(
        "INSERT INTO member_role_projects(actor_id,project_id) VALUES($1,$2)",
        &[&target.actor_id(), &hidden.id],
    )
    .unwrap();
    sql.execute("INSERT INTO developer_rating_entries(id,actor_id,date,source,score,reason,created_by,project_id) VALUES('hidden',$1,'2026-01-02','manager',5,'Private evidence',$2,$3)",&[&target.actor_id(),&f.ctx.actor_id(),&hidden.id]).unwrap();
    assert!(matches!(
        f.store.member_ratings(&peer, target.actor_id(), 2026),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.store.member_ratings(&target, target.actor_id(), 2026),
        Err(PlatformError::Forbidden)
    ));
    let private = f
        .store
        .member_ratings(&manager, target.actor_id(), 2026)
        .unwrap();
    assert!(
        day(&private, "2026-01-02")["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == "hidden")
    );
    let directory = f.store.rating_directory(&peer, None, 100).unwrap();
    assert!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == target.actor_id())
    );
    assert!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["id"] != manager.actor_id())
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM developer_rating_entries WHERE source='activity'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0,
        "reads must not close days or write global baselines"
    );
    let first = f.store.rating_directory(&peer, None, 1).unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["has_more"], true);
    let second = f
        .store
        .rating_directory(&peer, first["next_after"].as_str(), 1)
        .unwrap();
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    let role = f.store.member_role(&f.ctx, peer.actor_id()).unwrap();
    let revoked = f
        .store
        .create_project(&f.ctx, "Revoked peer", None)
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            peer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![revoked.id],
                expected_revision: role.revision,
            },
        )
        .unwrap();
    assert!(
        f.store
            .create_member_rating(&peer, target.actor_id(), &input)
            .is_err()
    );
    assert!(
        f.store
            .member_ratings(&outsider, target.actor_id(), 2026)
            .is_err()
    );
    assert!(
        f.store
            .member_ratings(&f.ctx, manager.actor_id(), 2026)
            .is_err()
    );
    f.store.refresh_developer_ratings().unwrap();
    assert!(matches!(
        f.store.member_ratings(&target, target.actor_id(), 2026),
        Err(PlatformError::Forbidden)
    ));
    assert!(
        f.store
            .member_ratings(&target, target.actor_id(), 1999)
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL; equal-weight hierarchical means, month boundaries, catchup and retroactive exclusions"]
fn aggregates_clip_month_weeks_and_recalculate_exclusions_without_duplicate_activity() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let target = member(
        &mut f,
        "rated",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let mut sql = connection(&f);
    // Approved leave excludes dates outside the four-day aggregation fixture.
    sql.batch_execute("UPDATE labor_calendar_settings SET starts_on='2026-01-01',workdays=ARRAY[1,2,3,4,5,6,7]; UPDATE actors SET created_at='2026-01-01'; UPDATE actor_roles SET rating_started_at='2026-01-01'").unwrap();
    sql.execute("INSERT INTO labor_leave_requests(id,actor_id,kind,start_date,end_date,reason,status) VALUES('rest',$1,'leave','2026-02-03','2026-12-31','fixture','approved'),('before',$1,'leave','2026-01-01','2026-01-29','fixture','approved')",&[&target.actor_id()]).unwrap();
    // Jan30/31 and Feb1 share an ISO week, Feb2 belongs to a new week.
    for (date, score) in [
        ("2026-01-30", 0.0),
        ("2026-01-31", 5.0),
        ("2026-02-01", 5.0),
        ("2026-02-02", 0.0),
    ] {
        sql.execute("INSERT INTO developer_rating_entries(id,actor_id,date,source,score,reason) VALUES($1,$2,$3::text::date,'activity',$4,'fixture')",&[&date,&target.actor_id(),&date,&score]).unwrap();
        if score == 5.0 {
            sql.execute("INSERT INTO developer_push_activity(actor_id,provider,scope_id,delivery_id,project_id,occurred_at) VALUES($1,'github','fixture',$2,$3,$2::text::date::timestamptz+interval '12 hour')",&[&target.actor_id(),&date,&project]).unwrap();
        }
    }
    // Extra peer entry affects daily mean only, each date/week/month receives equal weight above it.
    sql.execute("INSERT INTO developer_rating_entries(id,actor_id,date,source,score,reason,created_by,project_id) VALUES('peer',$1,'2026-01-30','peer',4,'review',$2,$3)",&[&target.actor_id(),&f.ctx.actor_id(),&project]).unwrap();
    f.store.refresh_developer_ratings().unwrap();
    let report = f
        .store
        .member_ratings(&f.ctx, target.actor_id(), 2026)
        .unwrap();
    assert_eq!(day(&report, "2026-01-30")["score"], 2.0);
    assert_eq!(report["monthly"][0]["score"], 3.5);
    assert_eq!(report["monthly"][1]["score"], 2.5);
    assert_eq!(report["annual"], 3.0);
    assert_eq!(report["weekly"][0]["score"], 4.0); // Jan26-Feb1: (2+5+5)/3
    assert_eq!(
        f.store.refresh_developer_ratings().unwrap(),
        0,
        "caught-up worker should not rewrite baseline entries"
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM developer_rating_entries WHERE actor_id=$1 AND source='activity'",
            &[&target.actor_id()]
        )
        .unwrap()
        .get::<_, i64>(0),
        4
    );
    sql.execute("INSERT INTO labor_leave_requests(id,actor_id,kind,start_date,end_date,minutes_per_day,reason,status) VALUES('hourly',$1,'leave','2026-01-31','2026-01-31',60,'hourly','approved')",&[&target.actor_id()]).unwrap();
    let partial = f
        .store
        .member_ratings(&f.ctx, target.actor_id(), 2026)
        .unwrap();
    assert_eq!(day(&partial, "2026-01-31")["score"], 5.0);
    sql.batch_execute("UPDATE labor_leave_requests SET minutes_per_day=480 WHERE id='hourly'")
        .unwrap();
    let full = f
        .store
        .member_ratings(&f.ctx, target.actor_id(), 2026)
        .unwrap();
    assert!(day(&full, "2026-01-31")["score"].is_null());
    assert_eq!(day(&full, "2026-01-31")["excluded"], true);
    assert_eq!(full["annual"], 2.25);
    sql.execute("INSERT INTO labor_holidays(id,name,date,status,proposed_by,reason) VALUES('holiday','Holiday','2026-02-01','approved',$1,'fixture')",&[&f.ctx.actor_id()]).unwrap();
    let holiday = f
        .store
        .member_ratings(&f.ctx, target.actor_id(), 2026)
        .unwrap();
    assert_eq!(day(&holiday, "2026-02-01")["excluded"], true);
}

#[test]
#[ignore = "requires PostgreSQL; signed push attribution uses linked stable IDs, never emails, with daily idempotence"]
fn verified_pushes_attribute_stable_accounts_and_ignore_claimed_authors() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use hmac::{Hmac, Mac};
    use nexora_platform_store::{GitNetworkPolicy, SecretGenerator, WebhookProof};
    use sha2::Sha256;
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let target = member(
        &mut f,
        "pusher",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let mut sql = connection(&f);
    let repo = f
        .store
        .register_repository(
            &f.ctx,
            &project,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let account = SecretGenerator::id("gcn").unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','pusher','active',1,'{}',now()+interval '1 hour')",&[&account,&target.actor_id()]).unwrap();
    sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2",&[&account,&repo.id]).unwrap();
    let mut key = zeroize::Zeroizing::new(vec![0u8; 32]);
    getrandom::fill(&mut key).unwrap();
    let policy = GitNetworkPolicy::default();
    let hook = f
        .store
        .register_gitlab_webhook(
            &f.ctx,
            &project,
            &repo.id,
            &GitlabWebhookRegistration {
                signing_token: format!("whsec_{}", STANDARD.encode(&*key)),
                expected_revision: 2,
            },
            &policy,
        )
        .unwrap();
    let delivery = |id: &str, user: u64| {
        let body=serde_json::to_vec(&json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":"a".repeat(40),"user_id":user,"user_email":"pusher@example.com","commits":[{"author":{"email":"pusher@example.com"}}]})).unwrap();
        let timestamp = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key).unwrap();
        mac.update(format!("{id}.{timestamp}.").as_bytes());
        mac.update(&body);
        (
            WebhookProof {
                delivery_id: id.into(),
                event: "Push Hook".into(),
                signature: format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes())),
                timestamp: Some(timestamp),
            },
            body,
        )
    };
    let (forged, body) = delivery("wrong-account", 999);
    f.store
        .receive_gitlab_webhook(&hook.id, &forged, &body, &policy)
        .unwrap();
    assert_eq!(
        sql.query_one("SELECT count(*) FROM developer_push_activity", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let (mut invalid, body) = delivery("invalid", 42);
    invalid.signature = "v1,invalid".into();
    assert!(
        f.store
            .receive_gitlab_webhook(&hook.id, &invalid, &body, &policy)
            .is_err()
    );
    for id in ["push-one", "push-two"] {
        let (proof, body) = delivery(id, 42);
        assert!(
            !f.store
                .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
                .unwrap()
                .duplicate
        );
        assert!(
            f.store
                .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
                .unwrap()
                .duplicate
        );
    }
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM developer_push_activity WHERE actor_id=$1",
            &[&target.actor_id()]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
    // Saturday fixture date is made a workday without assuming the test host's date.
    sql.batch_execute("UPDATE labor_calendar_settings SET workdays=ARRAY[1,2,3,4,5,6,7]")
        .unwrap();
    f.store.refresh_developer_ratings().unwrap();
    f.store.refresh_developer_ratings().unwrap();
    let row=sql.query_one("SELECT count(*),min(score) FROM developer_rating_entries WHERE actor_id=$1 AND source='activity'",&[&target.actor_id()]).unwrap();
    assert_eq!(row.get::<_, i64>(0), 1);
    assert_eq!(row.get::<_, Option<f64>>(1), Some(5.0));
    sql.execute(
        "UPDATE git_connections SET status='revoked' WHERE id=$1",
        &[&account],
    )
    .unwrap();
    let (proof, body) = delivery("revoked", 42);
    f.store
        .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
        .unwrap();
    assert_eq!(
        sql.query_one("SELECT count(*) FROM developer_push_activity", &[])
            .unwrap()
            .get::<_, i64>(0),
        2
    );
}

#[test]
#[ignore = "requires PostgreSQL; worker and calendar API acquire the shared calendar lock in the same order"]
fn concurrent_calendar_mutations_and_worker_closures_finish_without_deadlock() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    member(&mut f, "rated", MemberRole::Developer, vec![project]);
    let mut worker =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let ready = barrier.clone();
    let task = std::thread::spawn(move || {
        ready.wait();
        for _ in 0..12 {
            worker.refresh_developer_ratings().unwrap();
        }
    });
    barrier.wait();
    for _ in 0..12 {
        let settings = f.store.labor_calendar_settings(&f.ctx).unwrap();
        let input = json!({"timezone":settings["timezone"],"country":settings["country"],"workdays":settings["workdays"],"daily_minutes":settings["daily_minutes"],"starts_on":settings["starts_on"],"expected_revision":settings["revision"]});
        f.store.change_labor_calendar(&f.ctx, &input).unwrap();
    }
    task.join().unwrap();
}

#[test]
#[ignore = "requires PostgreSQL; member-role writes and peer reports lock target actors before target roles"]
fn concurrent_role_revocation_and_peer_report_do_not_invert_authority_locks() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let target = member(
        &mut f,
        "rated",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let peer = member(&mut f, "peer", MemberRole::Manager, vec![project]);
    let unrelated = f
        .store
        .create_project(&f.ctx, "Revoked scope", None)
        .unwrap();
    let mut sql = connection(&f);
    let writer_pid: i32 = sql
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    let mut tx = sql.transaction().unwrap();
    tx.batch_execute("SET LOCAL statement_timeout='4s'")
        .unwrap();
    tx.query_one(
        "SELECT id FROM actors WHERE id=$1 FOR UPDATE",
        &[&target.actor_id()],
    )
    .unwrap();
    let mut reader =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let target_id = target.actor_id().to_owned();
    let report = std::thread::spawn(move || reader.member_ratings(&peer, &target_id, 2026));
    // Observe the report waiting on the actor already held by the role writer.
    let mut monitor = connection(&f);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let blocked:bool=monitor.query_one("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) AND wait_event_type='Lock')",&[&writer_pid]).unwrap().get(0);
        if blocked {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "report never reached the target actor lock"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // This UPDATE deadlocks with the former session-wide role SHARE locks.
    tx.execute(
        "UPDATE actor_roles SET revision=revision+1 WHERE actor_id=$1",
        &[&target.actor_id()],
    )
    .unwrap();
    tx.execute(
        "DELETE FROM member_role_projects WHERE actor_id=$1",
        &[&target.actor_id()],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO member_role_projects(actor_id,project_id) VALUES($1,$2)",
        &[&target.actor_id(), &unrelated.id],
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(report.join().unwrap().is_ok());
}

#[test]
#[ignore = "requires PostgreSQL"]
fn personal_feedback_never_returns_raw_evaluations() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let dev = member(&mut f, "feedback", MemberRole::Developer, vec![project]);
    let cached = f.store.rating_feedback(&dev, 2026, false).unwrap();
    assert!(cached["report"].is_null());
    let input = f.store.rating_feedback(&dev, 2026, true).unwrap();
    assert_eq!(input.as_object().unwrap().len(), 3);
    assert!(input.get("daily").is_none());
    assert!(input.get("score").is_none());
    assert!(input.get("actor_id").is_none());
    assert!(f.store.rating_feedback(&dev, 2026, true).is_err());
    assert!(f.store.rating_feedback(&dev, 2025, true).is_err());
    assert!(f.store.rating_feedback(&f.ctx, 2026, true).is_err());
    let feedback = json!({"summary":"Continúa documentando tus avances.","strengths":[],"improvements":[],"actions":[]});
    f.store
        .finish_rating_feedback(&dev, 2026, &feedback)
        .unwrap();
    assert_eq!(
        f.store.rating_feedback(&dev, 2025, false).unwrap()["report"],
        feedback
    );
    assert!(
        f.store
            .finish_rating_feedback_for(&f.ctx, dev.actor_id(), 2026, &feedback)
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL; rating files stay private and outside the knowledge library"]
fn rating_evidence_is_bound_atomically_and_never_visible_to_the_rated_member() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let target = member(
        &mut f,
        "evidence_target",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let peer = member(
        &mut f,
        "evidence_peer",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let lead = member(
        &mut f,
        "evidence_lead",
        MemberRole::TechnicalLead,
        vec![project.clone()],
    );
    let mut sql = connection(&f);
    sql.batch_execute("UPDATE labor_calendar_settings SET starts_on='2026-01-01',workdays=ARRAY[1,2,3,4,5,6,7]; UPDATE actors SET created_at='2026-01-01'; UPDATE actor_roles SET rating_started_at='2026-01-01'").unwrap();
    assert!(
        f.store
            .upload_rating_evidence(
                &target,
                target.actor_id(),
                &project,
                "bad.pdf",
                false,
                b"data"
            )
            .is_err()
    );
    let file = f
        .store
        .upload_rating_evidence(
            &peer,
            target.actor_id(),
            &project,
            "review.pdf",
            false,
            b"%PDF example",
        )
        .unwrap();
    let id = file["id"].as_str().unwrap();
    assert!(f.store.rating_evidence_content(&peer, id).is_ok());
    assert!(f.store.rating_evidence_content(&lead, id).is_err());
    let input = json!({"date":"2026-01-02","project_id":project,"score":4.0,"reason":"**Reviewed** implementation","evidence_ids":[id]});
    f.store
        .create_member_rating(&peer, target.actor_id(), &input)
        .unwrap();
    assert!(f.store.rating_evidence_content(&target, id).is_err());
    assert!(f.store.rating_evidence_content(&lead, id).is_ok());
    let report = f
        .store
        .member_ratings(&lead, target.actor_id(), 2026)
        .unwrap();
    assert_eq!(
        day(&report, "2026-01-02")["entries"][0]["attachments"][0]["name"],
        "review.pdf"
    );
    assert!(
        f.store
            .create_member_rating(&peer, target.actor_id(), &input)
            .is_err()
    );
    let library = f
        .store
        .task_document_page(&f.ctx, None, Some(&project), "review", None)
        .unwrap();
    assert!(library["items"].as_array().unwrap().is_empty());
    f.store
        .change_member_role(
            &f.ctx,
            lead.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![project],
                expected_revision: 1,
            },
        )
        .unwrap();
    assert!(f.store.rating_evidence_content(&lead, id).is_err());
}

#[test]
#[ignore = "requires PostgreSQL; projectless ratings and private feedback"]
fn projectless_ratings_accept_files_without_exposing_private_evidence() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let target = member(
        &mut f,
        "global_target",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let peer = member(
        &mut f,
        "global_peer",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let lead = member(
        &mut f,
        "global_lead",
        MemberRole::TechnicalLead,
        vec![project],
    );
    let mut sql = connection(&f);
    sql.batch_execute("UPDATE labor_calendar_settings SET starts_on='2026-01-01',workdays=ARRAY[1,2,3,4,5,6,7]; UPDATE actors SET created_at='2026-01-01'; UPDATE actor_roles SET rating_started_at='2026-01-01'").unwrap();
    let file = f
        .store
        .upload_rating_evidence(
            &peer,
            target.actor_id(),
            "",
            "review.pdf",
            false,
            b"%PDF example",
        )
        .unwrap();
    let id = file["id"].as_str().unwrap();
    let input = json!({"date":"2026-01-02","project_id":null,"score":4.0,"reason":"**General collaboration**","evidence_ids":[id]});
    assert!(
        f.store
            .create_member_rating(&target, target.actor_id(), &input)
            .is_err()
    );
    let rating = f
        .store
        .create_member_rating(&peer, target.actor_id(), &input)
        .unwrap();
    assert!(rating["project_id"].is_null());
    assert!(f.store.rating_evidence_content(&target, id).is_err());
    assert!(f.store.rating_evidence_content(&lead, id).is_ok());
    assert!(
        f.store
            .rating_feedback_for(&peer, target.actor_id(), 2026, false)
            .is_err()
    );
    assert!(
        f.store
            .rating_feedback_for(&lead, target.actor_id(), 2026, false)
            .is_err()
    );
    assert!(
        f.store
            .rating_feedback_for(&f.ctx, target.actor_id(), 2026, false)
            .is_err()
    );
    assert!(
        f.store
            .rating_feedback_for(&lead, target.actor_id(), 2026, true)
            .is_err()
    );
    let signals = f
        .store
        .rating_feedback_for(&target, target.actor_id(), 2026, true)
        .unwrap();
    assert!(signals.get("daily").is_none());
    assert!(signals.get("reason").is_none());
    let report = f
        .store
        .member_ratings(&lead, target.actor_id(), 2026)
        .unwrap();
    assert_eq!(
        day(&report, "2026-01-02")["entries"][0]["attachments"][0]["name"],
        "review.pdf"
    );
}

#[test]
#[ignore = "requires PostgreSQL; own weekly aggregate must not disclose individual ratings"]
fn own_weekly_score_is_available_without_individual_entries() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let developer = member(
        &mut f,
        "weekly_developer",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let lead = member(
        &mut f,
        "weekly_lead",
        MemberRole::TechnicalLead,
        vec![project],
    );
    for actor in [&developer, &lead] {
        let value = f.store.own_weekly_rating(actor).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 3);
        assert!(
            object.contains_key("start")
                && object.contains_key("end")
                && object.contains_key("score")
        );
        assert!(
            f.store
                .member_ratings(actor, actor.actor_id(), 2026)
                .is_err()
        );
    }
    assert!(f.store.own_weekly_rating(&f.ctx).is_err());
}
