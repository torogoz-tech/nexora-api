use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::path::Path;

#[test]
#[ignore = "requires local PostgreSQL; self-service cannot escalate authority or recover secrets"]
fn personal_keys_are_self_scoped_and_observe_live_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_personal_mcp").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture").unwrap());
    store
        .bootstrap("Keys", "owner", &password, "Owner")
        .unwrap();
    let owner_session = store.login("owner", &password).unwrap();
    let ready = store
        .identity_gate(&owner_session.access_token, "skip", "")
        .unwrap();
    let owner = store
        .authenticate(ready["access_token"].as_str().unwrap())
        .unwrap();
    let project = store.create_project(&owner, "Allowed", None).unwrap();
    let other = store.create_project(&owner, "Other", None).unwrap();
    let member = store
        .create_user(&owner, "dev", &password, "Developer")
        .unwrap();
    store
        .change_member_role(
            &owner,
            &member.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = store.login("dev", &password).unwrap();
    let ready = store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let ctx = store
        .authenticate(ready["access_token"].as_str().unwrap())
        .unwrap();
    assert!(ctx.require(Capability::KeysManage, None).is_err());
    let mut input = PersonalMcpKeyCreate {
        name: "Development assistant".into(),
        project_ids: vec![project.id.clone()],
        read_only: false,
        include_global: false,
        expires_in_seconds: 86400,
    };
    let mut invalid_lifetime = input.clone();
    invalid_lifetime.expires_in_seconds = 7776001;
    assert!(matches!(
        store.issue_personal_mcp_key(&ctx, &invalid_lifetime),
        Err(PlatformError::Validation(_))
    ));
    invalid_lifetime.expires_in_seconds = 3599;
    assert!(matches!(
        store.issue_personal_mcp_key(&ctx, &invalid_lifetime),
        Err(PlatformError::Validation(_))
    ));
    let input_json = json!({"request":input});
    let issued = store
        .request(
            &ctx,
            "mcp.key.issue",
            "personal-key-once",
            &input_json,
            |s| s.issue_personal_mcp_key(&ctx, &input),
        )
        .unwrap();
    assert!(
        issued
            .metadata
            .scope
            .capabilities
            .contains(&Capability::TasksWrite)
    );
    assert!(
        !issued
            .metadata
            .scope
            .capabilities
            .contains(&Capability::KeysManage)
    );
    assert!(
        !issued
            .metadata
            .scope
            .capabilities
            .contains(&Capability::InstallationManage)
    );
    assert!(
        !issued
            .metadata
            .scope
            .capabilities
            .contains(&Capability::ApprovalsGrant)
    );
    assert!(issued.metadata.scope.installation);
    assert!(issued.metadata.scope.all_projects);
    assert!(issued.metadata.scope.project_ids.is_empty());
    assert!(matches!(
        store.request(
            &ctx,
            "mcp.key.issue",
            "personal-key-once",
            &input_json,
            |s| s.issue_personal_mcp_key(&ctx, &input)
        ),
        Err(PlatformError::Conflict(_))
    ));
    let api = store.authenticate(&issued.key).unwrap();
    assert!(matches!(
        store.personal_mcp_keys(&api),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        store.issue_personal_mcp_key(&api, &input),
        Err(PlatformError::Forbidden)
    ));
    let listed = store.personal_mcp_keys(&ctx).unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert!(listed["items"][0]["last_mcp_at"].is_null());
    assert!(listed["items"][0]["last_used_at"].is_string());
    assert!(!listed.to_string().contains(&issued.key));
    assert!(listed["items"][0].get("digest").is_none());
    store
        .record_mcp_activity(&api, Some(("Fixture client".into(), "1.0".into())))
        .unwrap();
    let activity = store.personal_mcp_keys(&ctx).unwrap();
    assert!(activity["items"][0]["last_mcp_at"].is_string());
    assert_eq!(activity["items"][0]["mcp_client_name"], "Fixture client");
    input.project_ids = vec![other.id.clone()];
    assert!(matches!(
        store.issue_personal_mcp_key(&ctx, &input),
        Err(PlatformError::Forbidden)
    ));
    input.project_ids = vec![project.id.clone()];
    input.include_global = true;
    assert!(api.scope().unwrap().installation);
    input.include_global = false;
    input.read_only = true;
    let readonly = store.issue_personal_mcp_key(&ctx, &input).unwrap();
    assert!(
        !readonly
            .metadata
            .scope
            .capabilities
            .contains(&Capability::TasksWrite)
    );
    let owner_key = store
        .issue_personal_mcp_key(
            &owner,
            &PersonalMcpKeyCreate {
                name: "Owner MCP".into(),
                project_ids: vec![project.id.clone()],
                read_only: false,
                include_global: true,
                expires_in_seconds: 86400,
            },
        )
        .unwrap();
    assert!(matches!(
        store.revoke_personal_mcp_key(&ctx, &owner_key.metadata.id),
        Err(PlatformError::NotFound)
    ));
    store
        .revoke_personal_mcp_key(&ctx, &issued.metadata.id)
        .unwrap();
    assert!(matches!(
        store.authenticate(&issued.key),
        Err(PlatformError::Unauthorized)
    ));
    // More than one history page, including keys created at adjacent timestamps.
    for index in 0..24 {
        input.name = format!("Historical fixture {index}");
        let key = store.issue_personal_mcp_key(&ctx, &input).unwrap();
        store
            .revoke_personal_mcp_key(&ctx, &key.metadata.id)
            .unwrap();
    }
    let query = PersonalMcpKeyListQuery {
        status: Some("inactive".into()),
        limit: Some(20),
        after: None,
    };
    let first = store.personal_mcp_key_page(&ctx, &query).unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    assert_eq!(first["has_more"], true);
    let second = store
        .personal_mcp_key_page(
            &ctx,
            &PersonalMcpKeyListQuery {
                after: Some(first["next_after"].as_str().unwrap().into()),
                ..query.clone()
            },
        )
        .unwrap();
    assert_eq!(second["items"].as_array().unwrap().len(), 5);
    assert_eq!(second["has_more"], false);
    let ids = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["items"].as_array().unwrap())
        .map(|v| v["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 25);
    assert!(!first.to_string().contains("digest"));
    assert!(matches!(
        store.personal_mcp_key_page(
            &ctx,
            &PersonalMcpKeyListQuery {
                limit: Some(21),
                ..query.clone()
            }
        ),
        Err(PlatformError::Validation(_))
    ));
    assert!(matches!(
        store.personal_mcp_key_page(
            &ctx,
            &PersonalMcpKeyListQuery {
                after: Some(owner_key.metadata.id.clone()),
                ..query.clone()
            }
        ),
        Err(PlatformError::Validation(_))
    ));
    let active = store
        .personal_mcp_key_page(
            &ctx,
            &PersonalMcpKeyListQuery {
                status: Some("active".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        active["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["revoked_at"].is_null())
    );
    assert_eq!(active["items"].as_array().unwrap().len(), 1);
    let previous_api = store.authenticate(&readonly.key).unwrap();
    // Role reduction affects issued credentials and new issuance immediately.
    store
        .change_member_role(
            &owner,
            &member.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![other.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    let stale_api = store.authenticate(&readonly.key).unwrap();
    assert!(store.get_project(&stale_api, &project.id).is_err());
    assert!(store.get_project(&stale_api, &other.id).is_ok());
    // The projects tool uses this same page: additions appear and removals disappear
    // without replacing the key, even when the caller retains an older context.
    let page = store.project_page(&previous_api, None, 100, false).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, other.id);
    assert!(
        stale_api
            .require(Capability::TasksWrite, Some(&other.id))
            .is_err()
    );
    input.read_only = false;
    assert!(matches!(
        store.issue_personal_mcp_key(&ctx, &input),
        Err(PlatformError::Forbidden)
    ));
    store
        .logout(ready["access_token"].as_str().unwrap())
        .unwrap();
    assert!(matches!(
        store.personal_mcp_keys(&ctx),
        Err(PlatformError::Unauthorized)
    ));
}
