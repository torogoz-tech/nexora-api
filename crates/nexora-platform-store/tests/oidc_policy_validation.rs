use nexora_platform::OidcProviderRegistration;
use nexora_platform_store::OidcNetworkPolicy;
#[test]
fn corporate_provider_policy_accepts_google_and_tenant_specific_entra_without_git_authority() {
    let policy = OidcNetworkPolicy::default();
    assert!(policy.validate().is_ok());
    for issuer in [
        "https://accounts.google.com",
        "https://login.microsoftonline.com/tenant-id/v2.0",
    ] {
        let input = OidcProviderRegistration {
            name: "Corporate IdP".into(),
            issuer: issuer.into(),
            client_id: "registered-application".into(),
            client_secret: nexora_platform_store::SecretGenerator::token("oidc_fixture").unwrap(),
            redirect_uri: "http://127.0.0.1:18080/api/v1/auth/oidc/callback".into(),
            enabled: true,
            expected_revision: 0,
        };
        assert!(policy.registration(&input).is_ok());
    }
    assert!(
        policy
            .provider_url("https://github.com/login/oauth/authorize")
            .is_err()
    );
}
#[test]
fn operator_origins_and_exact_callback_prevent_caller_selected_oidc_destinations() {
    let policy = OidcNetworkPolicy::default();
    for url in [
        "https://unknown.invalid",
        "http://accounts.google.com",
        "https://user:password@accounts.google.com",
        "https://accounts.google.com/#fragment",
    ] {
        assert!(policy.provider_url(url).is_err());
    }
    for callback in [
        "https://unknown.invalid/api/v1/auth/oidc/callback",
        "http://127.0.0.1:18080/another/path",
        "http://127.0.0.1:18080/api/v1/auth/oidc/callback?redirect=external",
    ] {
        assert!(policy.callback_url(callback).is_err());
    }
    assert!(
        policy
            .provider_url("https://www.googleapis.com/oauth2/v3/certs?application=id")
            .is_ok()
    );
    let custom = OidcNetworkPolicy {
        allowed_provider_origins: vec!["http://192.168.0.1".into()],
        allowed_callback_origins: vec!["http://127.0.0.1:18080".into()],
    };
    assert!(custom.validate().is_err());
}
