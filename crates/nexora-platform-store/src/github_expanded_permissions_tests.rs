use crate::github_client::GithubClient;
use serde_json::json;
#[test]
fn accepts_approved_app_permissions_but_keeps_code_tokens_read_only() {
    let approved = json!({"contents":"write","metadata":"read","pull_requests":"write","actions":"read","checks":"read","deployments":"read","administration":"write","statuses":"read"});
    assert!(GithubClient::app_permissions(&approved).is_ok());
    assert!(GithubClient::code_only_permissions(&approved).is_err());
    for extra in [
        json!({"contents":"admin"}),
        json!({"actions":"write"}),
        json!({"checks":"write"}),
        json!({"administration":"admin"}),
        json!({"statuses":"write"}),
        json!({"deployments":"write"}),
    ] {
        let mut permissions = approved.clone();
        permissions
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(GithubClient::app_permissions(&permissions).is_err());
    }
    assert!(
        GithubClient::code_only_permissions(
            &json!({"contents":"read","metadata":"read","actions":"read"})
        )
        .is_err()
    );
    assert!(
        GithubClient::code_only_permissions(
            &json!({"contents":"read","metadata":"read","checks":"read"})
        )
        .is_err()
    );
    assert!(
        GithubClient::code_only_permissions(&json!({"contents":"read","metadata":"read"})).is_ok()
    );
}
