#[test]
#[ignore = "requires local PostgreSQL and python3; stable GitLab remote identity"]
fn gitlab_rename_preserves_registered_identity_when_old_name_is_reused() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "rename",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "GitLab rename acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
