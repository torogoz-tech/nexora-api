use super::RepositoryBranchProtectionProvider as Provider;
use crate::{GitNetworkPolicy, pull_request_client::PullRequestClient};
use nexora_platform::{GitProvider, PlatformError};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
};

#[test]
fn preserves_stronger_github_controls_and_pinned_checks() {
    let current = json!({"required_status_checks":{"strict":true,"contexts":["security"],"checks":[{"context":"security","app_id":42}]},"required_pull_request_reviews":{"required_approving_review_count":3,"require_code_owner_reviews":true,"dismissal_restrictions":{"users":[{"login":"security"}],"teams":[],"apps":[]}},"restrictions":{"users":[{"login":"merger"}],"teams":[],"apps":[]},"required_signatures":{"enabled":true},"lock_branch":{"enabled":true}});
    let body = Provider::github_body(&current, &["build".into()]).unwrap();
    assert_eq!(
        body["required_pull_request_reviews"]["required_approving_review_count"],
        3
    );
    assert_eq!(
        body["required_pull_request_reviews"]["require_code_owner_reviews"],
        true
    );
    assert_eq!(body["required_status_checks"]["checks"][0]["app_id"], 42);
    assert_eq!(
        body["required_status_checks"]["checks"][1]["context"],
        "build"
    );
    assert_eq!(body["restrictions"]["users"], json!(["merger"]));
    assert_eq!(body["lock_branch"], true);
    assert_eq!(
        body["required_pull_request_reviews"]["bypass_pull_request_allowances"]["apps"],
        json!([])
    );
    assert_eq!(body["enforce_admins"], true);
}

#[test]
fn rejects_unrecognized_protections_instead_of_overwriting_them() {
    assert!(Provider::github_body(&json!({"future_restriction":true}), &["build".into()]).is_err());
}

#[test]
fn gitlab_deploy_keys_are_not_mistaken_for_no_push_access() {
    assert!(!Provider::gitlab_push_blocked(
        &json!({"push_access_levels":[{"access_level":0,"deploy_key_id":9}]})
    ));
    assert!(Provider::gitlab_push_blocked(
        &json!({"push_access_levels":[{"access_level":0,"id":1}]})
    ));
}

#[test]
fn github_requires_explicit_ci_before_network_access() {
    let client = PullRequestClient::new(&GitNetworkPolicy::default(), GitProvider::Github);
    assert!(matches!(
        Provider::preview(
            &client,
            GitProvider::Github,
            "/repos/org/repo",
            "fixture",
            &[]
        ),
        Err(PlatformError::Validation(_))
    ));
}

fn fixture(
    responses: Vec<(String, u16, Value)>,
    provider: GitProvider,
) -> (PullRequestClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for (path, status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = vec![0; 65536];
            let count = socket.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..count]);
            assert!(
                request.starts_with(&path),
                "Expected {path}, got {}",
                request.lines().next().unwrap()
            );
            let body = body.to_string();
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let policy = GitNetworkPolicy {
        github_api_origin: origin.clone(),
        gitlab_origin: origin,
        ..Default::default()
    };
    (PullRequestClient::new(&policy, provider), server)
}

#[test]
fn preflight_checks_every_branch_before_any_write() {
    let responses = ["main", "develop", "stage", "preprod"]
        .into_iter()
        .map(|branch| {
            (
                format!("GET /repos/org/repo/branches/{branch} "),
                if branch == "preprod" { 404 } else { 200 },
                json!({}),
            )
        })
        .collect();
    let (client, server) = fixture(responses, GitProvider::Github);
    assert!(matches!(
        Provider::apply(
            &client,
            GitProvider::Github,
            "/repos/org/repo",
            "fixture",
            &["build".into()]
        ),
        Err(PlatformError::Conflict(_))
    ));
    server.join().unwrap();
}

#[test]
fn previews_github_missing_rules_without_writes() {
    let mut responses = Vec::new();
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET /repos/org/repo/branches/{branch} "),
            200,
            json!({}),
        ));
    }
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET /repos/org/repo/branches/{branch}/protection "),
            404,
            json!({}),
        ));
    }
    let (client, server) = fixture(responses, GitProvider::Github);
    let plan = Provider::preview(
        &client,
        GitProvider::Github,
        "/repos/org/repo",
        "fixture",
        &["build".into()],
    )
    .unwrap();
    assert_eq!(plan["operations"].as_array().unwrap().len(), 4);
    assert_eq!(plan["verified"], false);
    server.join().unwrap();
}

#[test]
fn reconciles_ambiguous_github_write_by_readback_without_retrying() {
    let desired = Provider::github_body(&Value::Null, &["build".into()]).unwrap();
    let mut responses = Vec::new();
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET /repos/org/repo/branches/{branch} "),
            200,
            json!({}),
        ));
    }
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET /repos/org/repo/branches/{branch}/protection "),
            if branch == "main" { 404 } else { 200 },
            if branch == "main" {
                Value::Null
            } else {
                desired.clone()
            },
        ));
    }
    responses.push((
        "PUT /repos/org/repo/branches/main/protection ".into(),
        503,
        json!({}),
    ));
    responses.push((
        "GET /repos/org/repo/branches/main/protection ".into(),
        200,
        desired.clone(),
    ));
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET /repos/org/repo/branches/{branch} "),
            200,
            json!({}),
        ));
    }
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET /repos/org/repo/branches/{branch}/protection "),
            200,
            desired.clone(),
        ));
    }
    let (client, server) = fixture(responses, GitProvider::Github);
    let report = Provider::apply(
        &client,
        GitProvider::Github,
        "/repos/org/repo",
        "fixture",
        &["build".into()],
    )
    .unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["applied_operations"], 1);
    server.join().unwrap();
}

#[test]
fn gitlab_capability_failure_is_explicit_and_has_no_writes() {
    let root = "/api/v4/projects/42";
    let mut responses = Vec::new();
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET {root}/repository/branches/{branch} "),
            200,
            json!({}),
        ));
    }
    responses.push((
        format!("GET {root}/merge_requests?state=opened&per_page=100&page=1 "),
        200,
        json!([]),
    ));
    responses.push((
        format!("GET {root}/protected_branches?per_page=100&page=1 "),
        200,
        json!([]),
    ));
    responses.push((format!("GET {root} "),200,json!({"only_allow_merge_if_pipeline_succeeds":true,"allow_merge_on_skipped_pipeline":false})));
    responses.push((format!("GET {root}/approvals "), 404, json!({})));
    let (client, server) = fixture(responses, GitProvider::Gitlab);
    assert!(
        matches!(Provider::preview(&client,GitProvider::Gitlab,root,"fixture",&[]),Err(PlatformError::Conflict(message)) if message.contains("Premium/Ultimate"))
    );
    server.join().unwrap();
}

#[test]
fn gitlab_updates_existing_access_records_without_replacing_protections() {
    let root = "/api/v4/projects/42";
    let current = json!({"name":"main","allow_force_push":true,"push_access_levels":[{"id":2,"access_level":40},{"id":3,"access_level":0,"deploy_key_id":9}],"merge_access_levels":[{"id":4,"access_level":0}],"code_owner_approval_required":true});
    let mut responses = Vec::new();
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET {root}/repository/branches/{branch} "),
            200,
            json!({}),
        ));
    }
    responses.push((
        format!("GET {root}/merge_requests?state=opened&per_page=100&page=1 "),
        200,
        json!([]),
    ));
    responses.push((
        format!("GET {root}/protected_branches?per_page=100&page=1 "),
        200,
        json!([current.clone()]),
    ));
    responses.push((format!("GET {root} "),200,json!({"only_allow_merge_if_pipeline_succeeds":false,"allow_merge_on_skipped_pipeline":true})));
    responses.push((
        format!("GET {root}/approvals "),
        200,
        json!({"merge_requests_author_approval":true}),
    ));
    responses.push((
        format!("GET {root}/approval_rules?per_page=100&page=1 "),
        200,
        json!([{"name":"security","approvals_required":3}]),
    ));
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET {root}/protected_branches/{branch} "),
            200,
            current.clone(),
        ));
    }
    let (client, server) = fixture(responses, GitProvider::Gitlab);
    let plan = Provider::preview(&client, GitProvider::Gitlab, root, "fixture", &[]).unwrap();
    let operations = plan["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 7);
    let update = &operations[3];
    assert_eq!(update["method"], "PATCH");
    assert_eq!(
        update["body"]["allowed_to_push"],
        json!([{"id":2,"_destroy":true},{"id":3,"_destroy":true},{"access_level":0}])
    );
    assert!(update["body"].get("allowed_to_merge").is_none());
    assert!(update["body"].get("code_owner_approval_required").is_none());
    assert!(!operations.iter().any(|op| op["method"] == "DELETE"));
    server.join().unwrap();
}

#[test]
fn verifies_idempotent_gitlab_policy_without_any_writes() {
    let root = "/api/v4/projects/42";
    let current = json!({"name":"main","allow_force_push":false,"push_access_levels":[{"id":2,"access_level":0}],"merge_access_levels":[{"id":4,"access_level":40}],"code_owner_approval_required":true});
    let mut responses = Vec::new();
    for _ in 0..2 {
        for branch in ["main", "develop", "stage", "preprod"] {
            responses.push((
                format!("GET {root}/repository/branches/{branch} "),
                200,
                json!({}),
            ));
        }
        responses.push((
            format!("GET {root}/merge_requests?state=opened&per_page=100&page=1 "),
            200,
            json!([]),
        ));
        responses.push((
            format!("GET {root}/protected_branches?per_page=100&page=1 "),
            200,
            json!([current.clone()]),
        ));
        responses.push((format!("GET {root} "),200,json!({"only_allow_merge_if_pipeline_succeeds":true,"allow_merge_on_skipped_pipeline":false})));
        responses.push((format!("GET {root}/approvals "),200,json!({"merge_requests_author_approval":false,"merge_requests_disable_committers_approval":true,"disable_overriding_approvers_per_merge_request":true,"reset_approvals_on_push":true})));
        responses.push((format!("GET {root}/approval_rules?per_page=100&page=1 "),200,json!([{"name":"security","approvals_required":3},{"name":"Nexora independent approval","approvals_required":2,"applies_to_all_protected_branches":true}])));
        for branch in ["main", "develop", "stage", "preprod"] {
            responses.push((
                format!("GET {root}/protected_branches/{branch} "),
                200,
                current.clone(),
            ));
        }
    }
    let (client, server) = fixture(responses, GitProvider::Gitlab);
    let report = Provider::apply(&client, GitProvider::Gitlab, root, "fixture", &[]).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["applied_operations"], 0);
    assert!(!report["limitations"].as_array().unwrap().is_empty());
    server.join().unwrap();
}

#[test]
fn github_verification_accepts_new_implicit_app_binding_but_preserves_existing_pins() {
    let expected = Provider::github_body(&Value::Null, &["build".into()]).unwrap();
    let mut actual = expected.clone();
    actual["required_status_checks"]["checks"][0]["app_id"] = json!(123);
    assert!(Provider::github_verified(&actual, &expected).unwrap());
    let pinned = actual.clone();
    actual["required_status_checks"]["checks"][0]["app_id"] = json!(456);
    assert!(!Provider::github_verified(&actual, &pinned).unwrap());
    actual["required_status_checks"]["checks"][0]["app_id"] = json!(-1);
    assert!(!Provider::github_verified(&actual, &pinned).unwrap());
    assert!(!Provider::github_verified(&actual, &expected).unwrap());
}

#[test]
fn github_apply_verifies_provider_auto_bound_app_and_is_idempotent() {
    let root = "/repos/org/repo";
    let desired = Provider::github_body(&Value::Null, &["build".into()]).unwrap();
    let mut observed = desired.clone();
    observed["required_status_checks"]["checks"][0]["app_id"] = json!(123);
    let mut responses = Vec::new();
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((format!("GET {root}/branches/{branch} "), 200, json!({})));
    }
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET {root}/branches/{branch}/protection "),
            if branch == "main" { 404 } else { 200 },
            if branch == "main" {
                Value::Null
            } else {
                observed.clone()
            },
        ));
    }
    responses.push((
        format!("PUT {root}/branches/main/protection "),
        200,
        observed.clone(),
    ));
    responses.push((
        format!("GET {root}/branches/main/protection "),
        200,
        observed.clone(),
    ));
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((format!("GET {root}/branches/{branch} "), 200, json!({})));
    }
    for branch in ["main", "develop", "stage", "preprod"] {
        responses.push((
            format!("GET {root}/branches/{branch}/protection "),
            200,
            observed.clone(),
        ));
    }
    let (client, server) = fixture(responses, GitProvider::Github);
    let report = Provider::apply(
        &client,
        GitProvider::Github,
        root,
        "fixture",
        &["build".into()],
    )
    .unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["applied_operations"], 1);
    server.join().unwrap();
}

#[test]
fn gitlab_open_mr_with_overridden_or_missing_approval_rule_blocks_all_writes() {
    for state in [
        json!({"approval_rules_overwritten":true,"rules":[]}),
        json!({"approval_rules_overwritten":false,"rules":[{"approvals_required":0,"overridden":false}]}),
    ] {
        let root = "/api/v4/projects/42";
        let mut responses = Vec::new();
        for branch in ["main", "develop", "stage", "preprod"] {
            responses.push((
                format!("GET {root}/repository/branches/{branch} "),
                200,
                json!({}),
            ));
        }
        responses.push((
            format!("GET {root}/merge_requests?state=opened&per_page=100&page=1 "),
            200,
            json!([{"iid":7,"target_branch":"main"}]),
        ));
        responses.push((
            format!("GET {root}/merge_requests/7/approval_state "),
            200,
            state,
        ));
        let (client, server) = fixture(responses, GitProvider::Gitlab);
        assert!(
            matches!(Provider::apply(&client, GitProvider::Gitlab, root, "fixture", &[]), Err(PlatformError::Conflict(message)) if message.contains("Existing open merge requests"))
        );
        server.join().unwrap();
    }
}

#[test]
fn gitlab_open_mr_with_required_nonoverridden_approval_allows_preflight() {
    let root = "/api/v4/projects/42";
    let responses = vec![
        (
            format!("GET {root}/merge_requests?state=opened&per_page=100&page=1 "),
            200,
            json!([{"iid":7,"target_branch":"main"},{"iid":8,"target_branch":"feature"}]),
        ),
        (
            format!("GET {root}/merge_requests/7/approval_state "),
            200,
            json!({"approval_rules_overwritten":false,"rules":[{"approvals_required":3,"overridden":false}]}),
        ),
    ];
    let (client, server) = fixture(responses, GitProvider::Gitlab);
    assert!(Provider::gitlab_open_merge_requests(&client, root, "fixture").is_ok());
    server.join().unwrap();
}
