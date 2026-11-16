#[test]
#[ignore = "requires local PostgreSQL; actual executable, private temporary password file"]
fn cli_migrates_initializes_starts_and_restarts_without_leaking_credentials() {
    let output = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/cli_identity.py"
        ))
        .arg(env!("CARGO_BIN_EXE_nexora-api"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
