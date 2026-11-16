#[test]
#[ignore = "requires PostgreSQL and python3; scoped Standard Webhooks GitLab fixture"]
fn gitlab_webhooks_verify_signed_timestamp_body_and_scope_with_durable_deduplication() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "webhook",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "Gitlab webhook acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
