#[test]
#[ignore = "requires real local PostgreSQL, Dragonfly and Neo4j; public generated sources"]
fn mcp_clients_enrich_review_revise_and_withdraw_versioned_knowledge() {
    let output = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/mcp_knowledge.py"
        ))
        .arg(env!("CARGO_BIN_EXE_nexora"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
