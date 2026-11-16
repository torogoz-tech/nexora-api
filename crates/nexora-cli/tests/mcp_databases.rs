#[test]
#[ignore = "requires prepared local PostgreSQL, Dragonfly and Neo4j; run explicitly with --ignored"]
fn mcp_clients_index_restart_query_and_reuse_real_persistent_knowledge() {
    let output = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/mcp_live.py"
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
