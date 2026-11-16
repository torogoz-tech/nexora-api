#[test]
#[ignore = "requires PostgreSQL and uv; real MCP SDK against API executable with generated credentials"]
fn real_mcp_sdk_discovers_installation_and_enforces_project_isolation() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/mcp_client.py",
            env!("CARGO_BIN_EXE_nexora-api"),
        ])
        .output()
        .expect("uv must be installed for the opt-in SDK acceptance test");
    assert!(
        result.status.success(),
        "SDK acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
    assert_eq!(evidence["project_isolation"], true);
}
