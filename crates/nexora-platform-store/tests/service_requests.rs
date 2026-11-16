#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
fn member(f: &mut WorkItemFixture, name: &str, role: MemberRole) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture").unwrap());
    let a = f.store.create_user(&f.ctx, name, &password, name).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &a.id,
            &MemberRoleChange {
                role,
                all_projects: false,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login(name, &password).unwrap();
    let gate = f
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    f.store
        .authenticate(gate["access_token"].as_str().unwrap())
        .unwrap()
}
#[test]
#[ignore = "requires PostgreSQL"]
fn requests_isolate_ownership_technical_data_and_mcp_credentials() {
    let mut f = WorkItemFixture::new();
    let a = member(&mut f, "requester", MemberRole::User);
    let b = member(&mut f, "other-requester", MemberRole::User);
    let supervisor = member(&mut f, "supervisor", MemberRole::Supervisor);
    let product = f
        .store
        .save_product(
            &f.ctx,
            &json!({
                "name":"Customer portal", "project_ids":[f.project.id]
            }),
        )
        .unwrap();
    let catalog = f.store.products(&a, false).unwrap();
    assert_eq!(catalog["items"][0]["id"], product["id"]);
    assert!(catalog["items"][0].get("project_ids").is_none());
    assert!(matches!(
        f.store.products(&a, true),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.store.knowledge_reviews(&a, None, "pending", 1),
        Err(PlatformError::Forbidden)
    ));
    assert!(
        f.store
            .authorize_request_tool(&a, "nexora_request_products_list")
            .is_ok()
    );
    assert!(matches!(
        f.store.authorize_request_tool(&a, "nexora_products_list"),
        Err(PlatformError::Forbidden)
    ));
    let request=f.store.mutate_service_request(&a,None,"create",&json!({"title":"Printer issue","type_id":"support","body":"## Evidence\nPrinter offline"})).unwrap();
    let id = request["id"].as_str().unwrap();
    assert_eq!(f.store.service_requests(&a, "", "", 1).unwrap()["total"], 1);
    assert_eq!(f.store.service_requests(&b, "", "", 1).unwrap()["total"], 0);
    assert_eq!(
        f.store.service_requests(&supervisor, "", "", 1).unwrap()["total"],
        1
    );
    assert!(matches!(
        f.store.service_request(&b, id),
        Err(PlatformError::NotFound)
    ));
    assert!(
        f.store
            .service_request(&a, id)
            .unwrap()
            .get("links")
            .is_none()
    );
    assert!(matches!(
        f.store.mutate_service_request(
            &supervisor,
            Some(id),
            "change",
            &json!({"status":"completed","type_id":"support","expected_revision":1})
        ),
        Err(PlatformError::Forbidden)
    ));
    f.store
        .mutate_service_request(
            &supervisor,
            Some(id),
            "comment",
            &json!({"body":"Please check the cable."}),
        )
        .unwrap();
    assert_eq!(
        f.store.service_request(&a, id).unwrap()["comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for path in [
        "/api/v1/tasks",
        "/api/v1/projects",
        "/api/v1/settings",
        "/api/v1/knowledge",
    ] {
        assert!(matches!(
            f.store.authorize_request_workspace(&a, path),
            Err(PlatformError::Forbidden)
        ));
    }
    assert!(
        f.store
            .authorize_request_tool(&a, "nexora_request_list")
            .is_ok()
    );
    assert!(matches!(
        f.store.authorize_request_tool(&a, "nexora_projects_list"),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.store.get_project(&a, &f.project.id),
        Err(PlatformError::Forbidden | PlatformError::NotFound)
    ));
    let key = f
        .store
        .issue_personal_mcp_key(
            &a,
            &PersonalMcpKeyCreate {
                name: "Request assistant".into(),
                project_ids: vec![],
                read_only: true,
                include_global: true,
                expires_in_seconds: 86400,
            },
        )
        .unwrap();
    let agent = f.store.authenticate(&key.key).unwrap();
    assert_eq!(
        f.store.service_requests(&agent, "", "", 1).unwrap()["total"],
        1
    );
    assert!(matches!(
        f.store.mutate_service_request(
            &agent,
            None,
            "create",
            &json!({"title":"No","type_id":"support","body":"No"})
        ),
        Err(PlatformError::Forbidden)
    ));
    f.store
        .mutate_service_request(
            &f.ctx,
            Some(id),
            "change",
            &json!({"status":"in_progress","type_id":"incident","expected_revision":2}),
        )
        .unwrap();
    assert!(matches!(
        f.store.mutate_service_request(
            &f.ctx,
            Some(id),
            "change",
            &json!({"status":"completed","type_id":"incident","expected_revision":1})
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert!(matches!(
        f.store
            .attach_service_request(&a, id, "evil.html", b"<script>"),
        Err(PlatformError::Validation(_))
    ));
}

#[test]
#[ignore = "requires PostgreSQL"]
fn request_links_remain_internal_and_retries_do_not_duplicate_conversation() {
    let mut f = WorkItemFixture::new();
    let user = member(&mut f, "author", MemberRole::User);
    let r = f
        .store
        .mutate_service_request(
            &user,
            None,
            "create",
            &json!({"title":"Service","body":"Help","type_id":"service"}),
        )
        .unwrap();
    let id = r["id"].as_str().unwrap();
    let input = json!({"body":"**Evidence added**"});
    for _ in 0..2 {
        f.store
            .request(
                &user,
                "service_request.comment",
                "request-comment-1",
                &input,
                |store| store.mutate_service_request(&user, Some(id), "comment", &input),
            )
            .unwrap();
    }
    assert_eq!(
        f.store.service_request(&user, id).unwrap()["comments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let task = f.task("Internal investigation");
    f.store
        .link_service_request(
            &f.ctx,
            id,
            &json!({"kind":"task","target_id":task.id,"expected_revision":2}),
        )
        .unwrap();
    assert_eq!(
        f.store
            .linked_service_requests(&f.ctx, "task", &task.id)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let public = f.store.service_request(&user, id).unwrap();
    assert!(public.get("links").is_none());
    assert!(
        public["history"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["action"] != "linked")
    );
    assert!(matches!(
        f.store.linked_service_requests(&user, "task", &task.id),
        Err(PlatformError::Forbidden)
    ));
    let key = f
        .store
        .issue_personal_mcp_key(
            &user,
            &PersonalMcpKeyCreate {
                name: "MCP evidence".into(),
                project_ids: vec![],
                read_only: false,
                include_global: true,
                expires_in_seconds: 86400,
            },
        )
        .unwrap();
    let agent = f.store.authenticate(&key.key).unwrap();
    let file = f
        .store
        .attach_service_request(&agent, id, "notes.txt", b"Verified printer connection")
        .unwrap();
    let content = f
        .store
        .service_request_attachment(&agent, id, file["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(content.2, b"Verified printer connection");
    assert!(
        agent
            .scope()
            .unwrap()
            .capabilities
            .iter()
            .all(|c| matches!(c, Capability::RequestsRead | Capability::RequestsWrite))
    );
    assert!(
        f.store.personal_mcp_keys(&user).unwrap()["projects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn request_type_filter_precedes_pagination_and_respects_ownership() {
    let mut f = WorkItemFixture::new();
    let user = member(&mut f, "request-filter-user", MemberRole::User);
    let other = member(&mut f, "request-filter-other", MemberRole::User);
    f.store
        .mutate_service_request(
            &user,
            None,
            "create",
            &json!({"title":"Selected support","type_id":"support","body":"Help"}),
        )
        .unwrap();
    for index in 0..21 {
        f.store
            .mutate_service_request(
                &user,
                None,
                "create",
                &json!({"title":format!("Incident {index}"),"type_id":"incident","body":"Help"}),
            )
            .unwrap();
    }
    f.store
        .mutate_service_request(
            &other,
            None,
            "create",
            &json!({"title":"Private support","type_id":"support","body":"Help"}),
        )
        .unwrap();
    let filtered = f
        .store
        .service_requests_filtered(&user, "", "", "support", 1)
        .unwrap();
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["items"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["items"][0]["title"], "Selected support");
    assert_eq!(
        f.store
            .service_requests_filtered(&user, "", "", "incident", 2)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let types = f.store.service_request_types(&user).unwrap();
    assert_eq!(
        types["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == "support")
            .unwrap()["total"],
        1
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn sla_snapshot_attention_reports_and_ownership() {
    let mut f = WorkItemFixture::new();
    let user = member(&mut f, "sla-user", MemberRole::User);
    let other = member(&mut f, "sla-other", MemberRole::User);
    let supervisor = member(&mut f, "sla-supervisor", MemberRole::Supervisor);
    let types = f.store.service_request_types(&f.ctx).unwrap();
    let kind = types["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "support")
        .unwrap();
    let sla = json!({"critical":{"clock":"continuous","response_minutes":30,"resolution_minutes":120},"medium":{"clock":"business","response_minutes":60,"resolution_minutes":480}});
    f.store.save_service_request_type(&f.ctx,&json!({"id":"support","name":"Soporte","archived":false,"expected_revision":kind["revision"],"sla":sla})).unwrap();
    let request=f.store.mutate_service_request(&user,None,"create",&json!({"title":"=Potential CSV formula","body":"Private body never in reports","type_id":"support","criticality":"critical"})).unwrap();
    let id = request["id"].as_str().unwrap();
    assert_eq!(request["sla_snapshot"]["clock"], "continuous");
    assert!(request["first_attention_at"].is_null());
    let comment = f
        .store
        .mutate_service_request(&user, Some(id), "comment", &json!({"body":"Still waiting"}))
        .unwrap();
    assert!(comment["first_attention_at"].is_null());
    let attended = f
        .store
        .mutate_service_request(
            &f.ctx,
            Some(id),
            "comment",
            &json!({"body":"We are investigating"}),
        )
        .unwrap();
    assert!(attended["first_attention_at"].is_string());
    let changed=f.store.mutate_service_request(&f.ctx,Some(id),"change",&json!({"status":"completed","type_id":"support","expected_revision":attended["revision"]})).unwrap();
    assert!(changed["resolved_at"].is_string());
    let report = f.store.service_request_report(&user, &json!({})).unwrap();
    assert_eq!(report["total"], 1);
    assert_eq!(report["summary"]["completed"], 1);
    assert_eq!(report["items"][0]["sla"]["response_status"], "met");
    assert!(report["items"][0].get("body").is_none());
    assert_eq!(
        f.store
            .service_request_report(&other, &json!({"requester_id":user.actor_id()}))
            .unwrap()["total"],
        0
    );
    assert_eq!(
        f.store
            .service_request_report(&supervisor, &json!({"product_id":"none"}))
            .unwrap()["total"],
        1
    );
    assert_eq!(
        f.store
            .service_request_report(&supervisor, &json!({"type_id":"incident"}))
            .unwrap()["total"],
        0
    );
    assert!(
        f.store
            .authorize_request_tool(&supervisor, "nexora_request_report")
            .is_ok()
    );
    assert!(
        f.store
            .service_request_report(&user, &json!({"from":"2026-02-30"}))
            .is_err()
    );
    assert!(
        f.store
            .service_request_report(&user, &json!({"page":0}))
            .is_err()
    );
    let csv = nexora_platform_store::PlatformStore::service_request_report_csv(&report);
    assert!(csv.contains("'=Potential CSV formula"));
    assert!(!csv.contains("Private body"));
    let kinds = f.store.service_request_types(&f.ctx).unwrap();
    let kind = kinds["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "support")
        .unwrap();
    f.store.save_service_request_type(&f.ctx,&json!({"id":"support","name":"Soporte","archived":false,"expected_revision":kind["revision"],"sla":{}})).unwrap();
    assert_eq!(
        f.store.service_request(&user, id).unwrap()["sla"]["resolution_due_at"],
        request["sla_snapshot"]["resolution_due_at"]
    );
    assert!(f.store.save_service_request_type(&f.ctx,&json!({"name":"Invalid SLA","sla":{"critical":{"clock":"continuous","response_minutes":60,"resolution_minutes":10}}})).is_err());
}

#[test]
#[ignore = "requires PostgreSQL"]
fn sla_business_clock_holidays_overnight_and_continuous() {
    let f = WorkItemFixture::new();
    // Use the fixture's existing database configuration; private isolated schema.
    let mut c = postgres::Config::new();
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    c.host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database);
    let mut db = c.connect(postgres::NoTls).unwrap();
    db.batch_execute(&format!("SET search_path TO {}", f.namespace))
        .unwrap();
    let calendar = json!({"timezone":"America/El_Salvador","country":"SV","workdays":[1,2,3,4,5],"work_start":"08:00","work_end":"17:00"});
    let due: String = db
        .query_one(
            "SELECT request_sla_due('2026-11-13 22:30:00Z',60,$1,false)::text",
            &[&calendar],
        )
        .unwrap()
        .get(0);
    assert!(due.starts_with("2026-11-16 14:30"));
    db.execute("INSERT INTO labor_holidays(id,name,date,country,status,proposed_by,reason) VALUES('sla-holiday','Asueto','2026-11-16','SV','approved',$1,'')",&[&f.ctx.actor_id()]).unwrap();
    let due: String = db
        .query_one(
            "SELECT request_sla_due('2026-11-13 22:30:00Z',60,$1,false)::text",
            &[&calendar],
        )
        .unwrap()
        .get(0);
    assert!(due.starts_with("2026-11-17 14:30"));
    let due: String = db
        .query_one(
            "SELECT request_sla_due('2026-11-13 22:30:00Z',60,$1,true)::text",
            &[&calendar],
        )
        .unwrap()
        .get(0);
    assert!(due.starts_with("2026-11-13 23:30"));
    let night = json!({"timezone":"America/El_Salvador","country":"SV","workdays":[5],"work_start":"22:00","work_end":"06:00"});
    let due: String = db
        .query_one(
            "SELECT request_sla_due('2026-11-14 05:00:00Z',120,$1,false)::text",
            &[&night],
        )
        .unwrap()
        .get(0);
    assert!(due.starts_with("2026-11-14 07:00"));
}

#[test]
#[ignore = "requires PostgreSQL"]
fn request_type_appearance_is_validated_preserved_and_projected() {
    let mut f = WorkItemFixture::new();
    let requester = member(&mut f, "appearance-user", MemberRole::User);
    let kind = f
        .store
        .save_service_request_type(
            &f.ctx,
            &json!({"name":"Accesos","icon":"lock-keyhole","color":"#a78bfa"}),
        )
        .unwrap();
    assert_eq!(kind["icon"], "lock-keyhole");
    assert_eq!(kind["color"], "#A78BFA");
    let changed = f.store.save_service_request_type(&f.ctx, &json!({"id":kind["id"],"name":"Accesos","archived":true,"expected_revision":kind["revision"]})).unwrap();
    assert_eq!(changed["icon"], kind["icon"]);
    assert_eq!(changed["color"], kind["color"]);
    assert!(
        f.store
            .save_service_request_type(&f.ctx, &json!({"name":"Bad icon","icon":"unknown"}))
            .is_err()
    );
    assert!(
        f.store
            .save_service_request_type(&f.ctx, &json!({"name":"Bad color","color":"red;url(x)"}))
            .is_err()
    );
    assert!(
        f.store
            .save_service_request_type(
                &requester,
                &json!({"name":"Forbidden","icon":"bug","color":"#FFFFFF"})
            )
            .is_err()
    );
    let kinds = f.store.service_request_types(&requester).unwrap();
    let support = kinds["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "support")
        .unwrap();
    assert_eq!(support["icon"], "headset");
    assert_eq!(support["color"], "#38BDF8");
    let request = f
        .store
        .mutate_service_request(
            &requester,
            None,
            "create",
            &json!({"title":"Acceso","type_id":"support"}),
        )
        .unwrap();
    let detail = f
        .store
        .service_request(&requester, request["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(detail["type_icon"], "headset");
    assert_eq!(detail["type_color"], "#38BDF8");
}

#[test]
#[ignore = "requires PostgreSQL"]
fn product_filter_applies_before_pagination_and_preserves_request_ownership() {
    let mut f = WorkItemFixture::new();
    let a = member(&mut f, "product-requester", MemberRole::User);
    let b = member(&mut f, "product-other", MemberRole::User);
    let supervisor = member(&mut f, "product-supervisor", MemberRole::Supervisor);
    let product = f
        .store
        .save_product(&f.ctx, &json!({"name":"Portal","project_ids":[]}))
        .unwrap();
    let product_id = product["id"].as_str().unwrap();
    for i in 0..22 {
        f.store
            .mutate_service_request(
                &a,
                None,
                "create",
                &json!({"title":format!("Portal {i}"),"type_id":"support","product_id":product_id}),
            )
            .unwrap();
    }
    f.store
        .mutate_service_request(
            &a,
            None,
            "create",
            &json!({"title":"Without product","type_id":"support"}),
        )
        .unwrap();
    f.store
        .mutate_service_request(
            &b,
            None,
            "create",
            &json!({"title":"Other owner","type_id":"support","product_id":product_id}),
        )
        .unwrap();
    let first = f
        .store
        .service_requests_product_filtered(&a, "", "", "support", product_id, 1)
        .unwrap();
    assert_eq!(first["total"], 22);
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    assert!(
        first["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["product_id"] == product_id)
    );
    let second = f
        .store
        .service_requests_product_filtered(&a, "", "", "support", product_id, 2)
        .unwrap();
    assert_eq!(second["items"].as_array().unwrap().len(), 2);
    let all = f
        .store
        .service_requests_product_filtered(&supervisor, "", "", "", product_id, 1)
        .unwrap();
    assert_eq!(all["total"], 23);
    let none = f
        .store
        .service_requests_product_filtered(&a, "", "", "", "missing", 1)
        .unwrap();
    assert_eq!(none["total"], 0);
    let critical = f
        .store
        .service_requests_product_filtered_with_criticality(
            &a, "", "", "support", product_id, "medium", 1,
        )
        .unwrap();
    assert_eq!(critical["total"], 22);
}
