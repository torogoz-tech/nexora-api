#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; provider state lifecycle"]
fn github_configuration_can_toggle_without_secrets_and_requires_new_source_consent() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "state",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "GitHub state acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
