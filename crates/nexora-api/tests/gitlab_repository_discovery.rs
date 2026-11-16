#[test]
#[ignore = "requires local PostgreSQL and python3; scoped remote repository discovery"]
fn gitlab_repository_discovery_is_scoped_and_selection_pins_remote_identity() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "discovery",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "GitLab discovery acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
