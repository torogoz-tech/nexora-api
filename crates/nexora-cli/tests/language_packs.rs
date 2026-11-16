use std::{
    fs,
    process::{Command, Output},
};
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nexora"))
        .current_dir("../..")
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn discovered_packs_apply_one_policy_and_replay_without_native_specialization() {
    let listed = run(&["languages", "list"]);
    assert_eq!(listed.status.code(), Some(0));
    let text = String::from_utf8(listed.stdout).unwrap();
    assert!(text.contains("javascript"));
    assert!(text.contains("lua"));
    let fixtures = run(&[
        "policy",
        "test",
        "policies/architecture/language-neutral-boundary",
    ]);
    assert_eq!(
        fixtures.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixtures.stderr)
    );
    let results: serde_json::Value = serde_json::from_slice(&fixtures.stdout).unwrap();
    assert_eq!(results.as_array().unwrap().len(), 6);
    assert!(
        results
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["passed"] == true)
    );
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-neutral-packs-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let store = dir.join("reports");
    let export = dir.join("snapshot.json");
    let policy = "policies/architecture/language-neutral-boundary/boundary.policy.yaml";
    let checked = run(&[
        "governance",
        "check",
        "policies/architecture/language-neutral-boundary/fixtures/lua/pass",
        "--policies",
        policy,
        "--store",
        store.to_str().unwrap(),
        "--export-snapshot",
        export.to_str().unwrap(),
    ]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(first["language"], "lua");
    let replay = run(&[
        "governance",
        "check",
        "--snapshot",
        export.to_str().unwrap(),
        "--repository",
        first["repository_id"].as_str().unwrap(),
        "--policies",
        policy,
        "--store",
        store.to_str().unwrap(),
    ]);
    assert_eq!(
        replay.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&replay.stdout).unwrap()["id"],
        first["id"]
    );
    let template = run(&[
        "--language-dir",
        "templates/language-pack",
        "governance",
        "check",
        "tests/fixtures/template-repository",
        "--policies",
        policy,
        "--store",
        store.to_str().unwrap(),
    ]);
    assert_eq!(
        template.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&template.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&template.stdout).unwrap();
    assert_eq!(result["language"], "template");
    assert_eq!(result["status"], "FAIL");
    assert_eq!(result["policy_set_hash"], first["policy_set_hash"]);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn mixed_language_scope_requires_a_choice_instead_of_silently_skipping_a_language() {
    let result = run(&[
        "index",
        "--offline",
        "policies/architecture/language-neutral-boundary/fixtures",
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("select --language"));
}
