#[test]
#[ignore = "requires local PostgreSQL and fake GitLab provider; global profiles before project links and live source scope"]
fn gitlab_repository_profiles_verify_identity_scope_links_and_provider_requests() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "profile",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "GitLab profile acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
#[ignore = "requires local PostgreSQL and uv; GitHub profiles enforce installed App scope without a project"]
fn github_repository_profiles_verify_installation_scope_and_provider_requests() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/github_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "profile",
        ])
        .output()
        .expect("uv required");
    assert!(
        result.status.success(),
        "GitHub profile acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
