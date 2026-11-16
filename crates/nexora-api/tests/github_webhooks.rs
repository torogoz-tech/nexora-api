#[test]
#[ignore = "requires local PostgreSQL container/Docker and uv; authenticated local GitHub deliveries and real API"]
fn github_webhooks_authenticate_deduplicate_and_invalidate_only_bound_source() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "webhook",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "Github webhook acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
