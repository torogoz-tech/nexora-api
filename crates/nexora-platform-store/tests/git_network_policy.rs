use nexora_platform::GitlabClientRegistration;
use nexora_platform_store::GitNetworkPolicy;

#[test]
fn gitlab_operator_policy_confines_origin_and_exact_callback() {
    let policy = GitNetworkPolicy::default();
    assert!(policy.validate().is_ok());
    assert_eq!(policy.host().unwrap(), "gitlab.com");
    assert!(policy.url("https://elsewhere.example/api").is_err());
    assert!(policy.url("//elsewhere.example/api").is_err());
    for origin in [
        "http://gitlab.example",
        "https://user:password@gitlab.example",
        "https://gitlab.example/subdir",
        "https://gitlab.example?redirect=other",
        "https://gitlab.example#fragment",
    ] {
        let invalid = GitNetworkPolicy {
            gitlab_origin: origin.into(),
            ..policy.clone()
        };
        assert!(invalid.validate().is_err());
    }
    assert_eq!(policy.github_host().unwrap(), "github.com");
    assert!(policy.github_url("//elsewhere.example/api", true).is_err());
    assert!(
        policy
            .github_url("https://elsewhere.example/api", false)
            .is_err()
    );
    assert_eq!(
        policy.github_url("/app", true).unwrap().as_str(),
        "https://api.github.com/app"
    );
    let local = GitNetworkPolicy {
        gitlab_origin: "http://127.0.0.1:19090".into(),
        ..policy.clone()
    };
    assert!(local.validate().is_ok());
    assert_eq!(local.host().unwrap(), "127.0.0.1:19090");
    let mut registration = GitlabClientRegistration {
        client_id: "non-secret-test-id".into(),
        client_secret: nexora_platform_store::SecretGenerator::token("test").unwrap(),
        redirect_uri: "http://127.0.0.1:18080/api/v1/git/gitlab/callback".into(),
        enabled: true,
        expected_revision: 0,
    };
    assert!(policy.validate_registration(&registration).is_ok());
    registration.redirect_uri.push_str("?redirect=other");
    assert!(policy.validate_registration(&registration).is_err());
}
