#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; real API with signed GitHub App fixture"]
fn github_app_verifies_user_installation_and_repository_access_without_creating_login() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "GitHub acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
