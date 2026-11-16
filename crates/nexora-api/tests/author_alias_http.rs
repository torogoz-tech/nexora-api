#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; author alias human role/scope and MCP read contract"]
fn author_alias_http_enforces_scoped_coordinators_and_preserves_read_only_keys() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let mut actor = |username: &str, role: MemberRole| {
        let password = zeroize::Zeroizing::new(
            nexora_platform_store::SecretGenerator::token("alias_http").unwrap(),
        );
        let member = f
            .store
            .create_user(&f.ctx, username, &password, username)
            .unwrap();
        f.store
            .change_member_role(
                &f.ctx,
                &member.id,
                &MemberRoleChange {
                    role,
                    all_projects: false,
                    project_ids: vec![project.clone()],
                    expected_revision: 0,
                },
            )
            .unwrap();
        let session = f.store.login(username, &password).unwrap();
        (member.id, session.access_token)
    };
    let (developer, developer_token) = actor("alias_http_developer", MemberRole::Developer);
    let (other, _) = actor("alias_http_other", MemberRole::Developer);
    let (manager, manager_token) = actor("alias_http_manager", MemberRole::Manager);
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &developer,
            "Scoped author reader",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let manager_key = f
        .store
        .issue_key(
            &f.ctx,
            &manager,
            "Coordinator key remains read only",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request =
        |method: &str, path: &str, token: &str, body: Option<Value>, receipt: Option<&str>| {
            runtime.block_on(async {
                let mut builder = Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"))
                    .header("accept", "application/json, text/event-stream");
                if body.is_some() {
                    builder = builder.header("content-type", "application/json");
                }
                if let Some(id) = receipt {
                    builder = builder.header("idempotency-key", id);
                }
                let response =
                    app.clone()
                        .oneshot(
                            builder
                                .body(body.map_or_else(Body::empty, |value| {
                                    Body::from(value.to_string())
                                }))
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                (status, serde_json::from_slice::<Value>(&bytes).unwrap())
            })
        };
    let path = format!("/api/v1/members/{developer}/git-aliases");
    let input = json!({"provider":"github","host":"github.com","email":"Dev@Example.COM","project_id":project,"reason":"Associate reviewed work email"});
    let (status, created) = request(
        "POST",
        &path,
        &manager_token,
        Some(input.clone()),
        Some("alias-http-create"),
    );
    assert!(status.is_success(), "{status}: {created}");
    assert_eq!(created["actor_id"], developer);
    assert_eq!(created["email"], "dev@example.com");
    assert_eq!(
        request(
            "POST",
            &path,
            &manager_token,
            Some(input.clone()),
            Some("alias-http-create")
        )
        .1,
        created
    );
    let mut changed_input = input.clone();
    changed_input["email"] = json!("different@example.com");
    assert_eq!(
        request(
            "POST",
            &path,
            &manager_token,
            Some(changed_input.clone()),
            Some("alias-http-create")
        )
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            "POST",
            &path,
            &manager_key.key,
            Some(changed_input),
            Some("alias-http-key-denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let mut self_input = input.clone();
    self_input["email"] = json!("second@example.com");
    let (status, denied) = request(
        "POST",
        &path,
        &developer_token,
        Some(self_input),
        Some("alias-http-self-denied"),
    );
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
    let (status, denied) = request(
        "POST",
        &path,
        &manager_token,
        Some(
            json!({"provider":"github","host":"github.com","email":"global@example.com","project_id":null,"reason":"Attempt outside project scope"}),
        ),
        Some("alias-http-global-denied"),
    );
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
    let (status, conflict) = request(
        "POST",
        &format!("/api/v1/members/{other}/git-aliases"),
        &manager_token,
        Some(input),
        Some("alias-http-collision"),
    );
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    let (status, listed) = request(
        "GET",
        &format!("{path}?project_id={project}"),
        &developer_token,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alias| alias["id"] == created["id"])
    );
    let (status, mcp) = request(
        "POST",
        "/mcp",
        &key.key,
        Some(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_git_author_aliases_list","arguments":{"query":{"project_id":project,"member_id":developer}}}}),
        ),
        None,
    );
    assert_eq!(status, StatusCode::OK, "{mcp}");
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    assert!(mcp.to_string().contains("dev@example.com"));
    let alias_id = created["id"].as_str().unwrap();
    let revoke_path = format!("{path}/{alias_id}/revoke");
    let revoke = json!({"expected_revision":created["revision"],"reason":"Stop future attribution for retired email"});
    let (status, revoked) = request(
        "POST",
        &revoke_path,
        &manager_token,
        Some(revoke.clone()),
        Some("alias-http-revoke"),
    );
    assert_eq!(status, StatusCode::OK, "{revoked}");
    assert!(revoked["revoked_at"].is_string());
    assert_eq!(
        request(
            "POST",
            &revoke_path,
            &manager_token,
            Some(revoke.clone()),
            Some("alias-http-revoke")
        )
        .1,
        revoked
    );
    f.store
        .change_member_role(
            &f.ctx,
            &manager,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![project],
                expected_revision: 1,
            },
        )
        .unwrap();
    assert_eq!(
        request(
            "POST",
            &revoke_path,
            &manager_token,
            Some(revoke),
            Some("alias-http-revoke")
        )
        .0,
        StatusCode::FORBIDDEN
    );
}
