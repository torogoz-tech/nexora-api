#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; GitHub configuration and connection catalog"]
fn github_configuration_metadata_and_connection_catalog_respect_actor_authority() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "catalog",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "GitHub catalog acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
