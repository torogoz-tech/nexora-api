use std::{
    fs,
    path::Path,
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
fn governance_commands_preserve_exit_codes_evidence_and_snapshot_replay() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-gov-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let store = dir.join("store");
    let store = store.to_str().unwrap();
    let export = dir.join("snapshot.json");
    let export = export.to_str().unwrap();
    assert_eq!(
        run(&["policy", "validate", "policies/examples"])
            .status
            .code(),
        Some(0)
    );
    let fixture = run(&["policy", "test", "policies/examples"]);
    assert_eq!(
        fixture.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixture.stderr)
    );
    let cases: serde_json::Value = serde_json::from_slice(&fixture.stdout).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 3);
    let pass = run(&[
        "governance",
        "check",
        "policies/examples/fixtures/naming/pass",
        "--policies",
        "policies/examples",
        "--store",
        store,
        "--export-snapshot",
        export,
    ]);
    assert_eq!(
        pass.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&pass.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&pass.stdout).unwrap();
    assert_eq!(first["status"], "PASS");
    assert_eq!(first["findings"][0]["evidence"][0]["path"], "source.js");
    let report = run(&["governance", "report", "--store", store, "--format", "json"]);
    assert_eq!(report.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&report.stdout).unwrap(),
        first
    );
    let replay = run(&[
        "governance",
        "check",
        "--snapshot",
        export,
        "--repository",
        first["repository_id"].as_str().unwrap(),
        "--policies",
        "policies/examples",
        "--store",
        store,
    ]);
    assert_eq!(
        replay.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let replay: serde_json::Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(first["id"], replay["id"]);
    let denied = run(&[
        "governance",
        "check",
        "--snapshot",
        export,
        "--repository",
        "different",
        "--policies",
        "policies/examples",
        "--store",
        store,
    ]);
    assert_eq!(denied.status.code(), Some(2));
    let fail = run(&[
        "governance",
        "check",
        "policies/examples/fixtures/naming/fail",
        "--policies",
        "policies/examples",
        "--store",
        store,
    ]);
    assert_eq!(fail.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fail.stdout).unwrap()["status"],
        "FAIL"
    );
    let text = run(&["governance", "report", "--store", store, "--format", "text"]);
    assert!(String::from_utf8_lossy(&text.stdout).contains("source.js:1:1"));
    let unknown = run(&[
        "governance",
        "check",
        "policies/examples/fixtures/naming/unknown",
        "--policies",
        "policies/examples",
        "--store",
        store,
        "--unknown",
        "fail",
    ]);
    assert_eq!(unknown.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&unknown.stdout).unwrap()["status"],
        "UNKNOWN"
    );
    assert_eq!(
        run(&["policy", "validate", "missing-directory"])
            .status
            .code(),
        Some(2)
    );
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn policy_fixture_mismatch_and_path_escape_are_detected() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-gov-fixture-cli-{}", std::process::id()));
    fs::create_dir_all(dir.join("source")).unwrap();
    fs::write(dir.join("source/a.js"), "class Good {}").unwrap();
    fs::copy(
        Path::new("../../policies/examples/naming.policy.yaml"),
        dir.join("rule.policy.yaml"),
    )
    .unwrap();
    let suite = "apiVersion: nexora.dev/v1alpha1\nkind: PolicyTestSuite\ncases:\n  - name: incorrect expectation\n    policy: EXAMPLE-CLASS-NAMES\n    source: source\n    expected: FAIL\n";
    fs::write(dir.join("rule.tests.yaml"), suite).unwrap();
    assert_eq!(
        run(&["policy", "test", dir.to_str().unwrap()])
            .status
            .code(),
        Some(1)
    );
    fs::write(
        dir.join("rule.tests.yaml"),
        suite.replace("source: source", "source: ../outside"),
    )
    .unwrap();
    assert_eq!(
        run(&["policy", "test", dir.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_rejects_json_expansion_that_cannot_be_reimported() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-gov-export-budget-{}", std::process::id()));
    fs::create_dir_all(dir.join("source")).unwrap();
    let mut content = String::from("//");
    content.push_str(&"\u{1}".repeat(1_048_574));
    for n in 0..12 {
        fs::write(dir.join(format!("source/a{n}.js")), &content).unwrap();
    }
    let export = dir.join("too-large.json");
    let store = dir.join("store");
    let result = run(&[
        "governance",
        "check",
        dir.join("source").to_str().unwrap(),
        "--policies",
        "policies/examples",
        "--store",
        store.to_str().unwrap(),
        "--export-snapshot",
        export.to_str().unwrap(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!export.exists());
    fs::remove_dir_all(dir).unwrap();
}
