#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; local GitHub App manifest/provider fixture"]
fn github_manifest_configures_app_after_correlated_one_use_callback_and_restart() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "manifest",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "Manifest acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
