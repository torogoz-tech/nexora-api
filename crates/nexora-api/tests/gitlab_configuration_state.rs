#[test]
#[ignore = "requires local PostgreSQL and python3; provider state lifecycle"]
fn gitlab_configuration_can_toggle_without_secrets_and_requires_new_source_consent() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "state",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "GitLab state acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
