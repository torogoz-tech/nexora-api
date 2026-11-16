use nexora_platform::{NavigationOrderChange, PlatformError};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::path::Path;
#[test]
#[ignore = "requires local PostgreSQL; common menu order never grants authority"]
fn navigation_order_is_shared_revisioned_and_only_administrator_can_change_it() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let schema = SecretGenerator::id("nexora_platform_navigation").unwrap();
    let mut store = PlatformStore::connect(&config, &schema).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Navigation", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let verified = store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let owner = store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap();
    store
        .create_user(&owner, "reader", &password, "Reader")
        .unwrap();
    let reader_session = store.login("reader", &password).unwrap();
    let verified = store
        .identity_gate(&reader_session.access_token, "skip", "")
        .unwrap();
    let reader_token = verified["access_token"].as_str().unwrap();
    let reader = store.authenticate(reader_token).unwrap();
    let initial = store.navigation_order(&reader).unwrap();
    assert_eq!(initial["revision"], 0);
    assert_eq!(initial["can_manage"], false);
    let mut paths: Vec<String> = serde_json::from_value(initial["paths"].clone()).unwrap();
    assert_eq!(
        &paths[..4],
        &[
            "/settings",
            "/settings/members",
            "/settings/products",
            "/settings/projects"
        ]
    );
    paths.reverse();
    let input = NavigationOrderChange {
        expected_revision: 0,
        paths: paths.clone(),
    };
    assert!(matches!(
        store.change_navigation_order(&reader, &input),
        Err(PlatformError::Forbidden)
    ));
    let changed = store.change_navigation_order(&owner, &input).unwrap();
    assert_eq!(changed["revision"], 1);
    assert_eq!(
        store.navigation_order(&reader).unwrap()["paths"],
        changed["paths"]
    );
    assert!(matches!(
        store.change_navigation_order(&owner, &input),
        Err(PlatformError::Conflict(_))
    ));
    for paths in [
        vec!["/settings".into(), "/settings".into()],
        vec!["/admin/untrusted".into()],
    ] {
        assert!(matches!(
            store.change_navigation_order(
                &owner,
                &NavigationOrderChange {
                    expected_revision: 1,
                    paths
                }
            ),
            Err(PlatformError::Validation(_))
        ));
    }
    store.logout(reader_token).unwrap();
    assert!(matches!(
        store.navigation_order(&reader),
        Err(PlatformError::Unauthorized)
    ));
}
