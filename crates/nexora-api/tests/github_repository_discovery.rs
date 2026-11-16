#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; scoped remote repository discovery"]
fn github_repository_discovery_is_scoped_and_selection_pins_remote_identity() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "discovery",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "GitHub discovery acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
