#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; stable GitHub remote identity"]
fn github_rename_preserves_registered_identity_when_old_name_is_reused() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "rename",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "GitHub rename acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
