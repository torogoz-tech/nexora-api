use crate::github_client::GithubClient;
use serde_json::json;
#[test]
fn accepts_governed_app_writes_while_code_fetch_tokens_remain_read_only() {
    assert!(GithubClient::app_permissions(&json!({"contents":"read","metadata":"read"})).is_ok());
    assert!(
        GithubClient::app_permissions(
            &json!({"contents":"read","metadata":"read","pull_requests":"read"})
        )
        .is_ok()
    );
    assert!(
        GithubClient::code_only_permissions(&json!({"contents":"read","metadata":"read"})).is_ok()
    );
    assert!(
        GithubClient::code_only_permissions(
            &json!({"contents":"read","metadata":"read","pull_requests":"read"})
        )
        .is_err()
    );
    for extra in [json!({"issues":"read"}), json!({"administration":"admin"})] {
        let mut permissions = json!({"contents":"read","metadata":"read"});
        permissions
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(GithubClient::app_permissions(&permissions).is_err());
    }
    assert!(
        GithubClient::app_permissions(
            &json!({"contents":"write","metadata":"read","pull_requests":"read"})
        )
        .is_ok()
    );
    assert!(
        GithubClient::app_permissions(&json!({"metadata":"read","pull_requests":"read"})).is_err()
    );
}

#[test]
fn writable_contents_are_never_accepted_for_code_only_tokens() {
    assert!(
        GithubClient::code_only_permissions(&json!({"contents":"write","metadata":"read"}))
            .is_err()
    );
}
