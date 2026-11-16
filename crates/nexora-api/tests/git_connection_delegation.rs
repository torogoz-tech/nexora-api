#[test]
#[ignore = "requires local PostgreSQL and python3; explicit project delegation withdrawal"]
fn withdrawing_connection_delegation_revokes_only_its_project_and_replays_safely() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "delegation",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "Delegation acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
