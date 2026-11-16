#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::{Value, json};
use work_item_fixture::WorkItemFixture;

fn member(
    f: &mut WorkItemFixture,
    name: &str,
    role: MemberRole,
    projects: Vec<String>,
) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("labor_fallback_test").unwrap());
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
    f.store.authenticate(&session.access_token).unwrap()
}
fn request(f: &mut WorkItemFixture, ctx: &AuthContext, day: &str) -> Value {
    f.store
        .create_labor_leave_request(
            ctx,
            &json!({
                "kind":"leave", "start_date":day, "end_date":day,
                "minutes_per_day":60, "reason":"Family appointment",
            }),
        )
        .unwrap()
}
fn pending(data: Value, id: &str) -> bool {
    data["requests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == id && item["can_decide"] == true)
}

#[test]
#[ignore = "requires local PostgreSQL; unassigned fallback, assigned precedence and live roles"]
fn unassigned_requests_allow_managers_and_technical_leads_without_project_coverage() {
    let mut f = WorkItemFixture::new();
    let own = f.project.id.clone();
    let outside = f.store.create_project(&f.ctx, "Outside", None).unwrap().id;
    let developer = member(
        &mut f,
        "developer",
        MemberRole::Developer,
        vec![own.clone()],
    );
    let manager = member(
        &mut f,
        "outside_manager",
        MemberRole::Manager,
        vec![outside.clone()],
    );
    let lead = member(
        &mut f,
        "outside_lead",
        MemberRole::TechnicalLead,
        vec![outside.clone()],
    );
    let pmo = member(&mut f, "pmo", MemberRole::Pmo, vec![own.clone()]);
    let colleague = member(
        &mut f,
        "colleague",
        MemberRole::Developer,
        vec![own.clone()],
    );
    let first = request(&mut f, &developer, "2026-12-01");
    let id = first["id"].as_str().unwrap();
    assert!(pending(f.store.labor_calendar(&manager).unwrap(), id));
    assert!(pending(f.store.labor_calendar(&lead).unwrap(), id));
    assert!(!pending(f.store.labor_calendar(&pmo).unwrap(), id));
    assert!(!pending(f.store.labor_calendar(&colleague).unwrap(), id));
    assert!(matches!(
        f.store
            .decide_labor_request(&developer, id, &json!({"approved":true,"reason":"Self"})),
        Err(PlatformError::Forbidden)
    ));
    f.store
        .decide_labor_request(&manager, id, &json!({"approved":true,"reason":"Approved"}))
        .unwrap();
    let second = request(&mut f, &developer, "2026-12-02");
    f.store
        .decide_labor_request(
            &lead,
            second["id"].as_str().unwrap(),
            &json!({"approved":true,"reason":"Approved"}),
        )
        .unwrap();
    // Explicit assignment restricts fallback and may include a technical lead.
    let assigned_lead = member(
        &mut f,
        "assigned_lead",
        MemberRole::TechnicalLead,
        vec![own.clone()],
    );
    f.store
        .assign_labor_approver(
            &f.ctx,
            developer.actor_id(),
            &json!({"approver_id":assigned_lead.actor_id(),"expected_revision":0}),
        )
        .unwrap();
    let third = request(&mut f, &developer, "2026-12-03");
    let third_id = third["id"].as_str().unwrap();
    assert!(!pending(
        f.store.labor_calendar(&manager).unwrap(),
        third_id
    ));
    assert!(matches!(
        f.store.decide_labor_request(
            &lead,
            third_id,
            &json!({"approved":true,"reason":"Not assigned"})
        ),
        Err(PlatformError::Forbidden)
    ));
    assert!(pending(
        f.store.labor_calendar(&assigned_lead).unwrap(),
        third_id
    ));
    let lead_request = request(&mut f, &assigned_lead, "2026-12-04");
    assert!(matches!(
        f.store.decide_labor_request(
            &assigned_lead,
            lead_request["id"].as_str().unwrap(),
            &json!({"approved":true,"reason":"Self lead"})
        ),
        Err(PlatformError::Forbidden)
    ));
    let key = f
        .store
        .issue_key(
            &f.ctx,
            assigned_lead.actor_id(),
            "Calendar API cannot approve",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![own.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&key.key).unwrap();
    assert!(matches!(
        f.store
            .decide_labor_request(&api, third_id, &json!({"approved":true,"reason":"API key"})),
        Err(PlatformError::Forbidden)
    ));
    // A cleared assignment (NULL row) behaves exactly like no row.
    f.store
        .assign_labor_approver(
            &f.ctx,
            developer.actor_id(),
            &json!({"approver_id":null,"expected_revision":1}),
        )
        .unwrap();
    assert!(pending(f.store.labor_calendar(&manager).unwrap(), third_id));
    // Earlier authenticated contexts never retain a removed approval role.
    f.store
        .change_member_role(
            &f.ctx,
            manager.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![outside],
                expected_revision: 1,
            },
        )
        .unwrap();
    assert!(matches!(
        f.store.decide_labor_request(
            &manager,
            third_id,
            &json!({"approved":true,"reason":"Stale role"})
        ),
        Err(PlatformError::Forbidden)
    ));
    f.store.suspend_actor(&f.ctx, lead.actor_id()).unwrap();
    assert!(
        f.store
            .decide_labor_request(
                &lead,
                third_id,
                &json!({"approved":true,"reason":"Suspended"})
            )
            .is_err()
    );
}

#[test]
#[ignore = "requires local PostgreSQL; initial approver assignment serializes against a fallback decision"]
fn concurrent_assignment_prevents_a_waiting_fallback_decision() {
    use nexora_platform_store::PlatformStore;
    use std::time::{Duration, Instant};
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let developer = member(
        &mut f,
        "developer",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let assigned = member(
        &mut f,
        "assigned",
        MemberRole::Manager,
        vec![project.clone()],
    );
    let outsider = member(&mut f, "fallback", MemberRole::Manager, vec![project]);
    let request = request(&mut f, &developer, "2026-12-10");
    let id = request["id"].as_str().unwrap().to_owned();
    assert!(pending(f.store.labor_calendar(&outsider).unwrap(), &id));
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
    let mut assigning = sql.transaction().unwrap();
    assigning
        .query_one(
            "SELECT singleton FROM labor_calendar_settings WHERE singleton FOR UPDATE",
            &[],
        )
        .unwrap();
    assigning
        .execute(
            "INSERT INTO labor_approvers(actor_id,approver_id) VALUES($1,$2)",
            &[&developer.actor_id(), &assigned.actor_id()],
        )
        .unwrap();
    let xid: String = assigning
        .query_one("SELECT pg_current_xact_id()::text", &[])
        .unwrap()
        .get(0);
    let mut concurrent = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let worker = std::thread::spawn(move || {
        concurrent.decide_labor_request(
            &outsider,
            &id,
            &json!({"approved":true,"reason":"Waiting fallback"}),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let waiting:bool=assigning.query_one("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='transactionid' AND transactionid::text=$1 AND NOT granted)",&[&xid]).unwrap().get(0);
        if waiting {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "fallback decision must wait for the assignment's calendar lock"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assigning.commit().unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(PlatformError::Forbidden)
    ));
}
