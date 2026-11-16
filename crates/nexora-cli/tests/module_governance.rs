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
fn architecture_policy_cli_preserves_evidence_coverage_and_replay() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-local-modules-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let store = dir.join("reports");
    let store = store.to_str().unwrap();
    let exported = dir.join("snapshot.json");
    let exported = exported.to_str().unwrap();
    let tests = run(&["policy", "test", "policies/architecture/domain-boundary"]);
    assert_eq!(
        tests.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&tests.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&tests.stdout)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        9
    );
    let checked = run(&[
        "governance",
        "check",
        "policies/architecture/domain-boundary/fixtures/pass",
        "--policies",
        "policies/architecture/domain-boundary",
        "--store",
        store,
        "--export-snapshot",
        exported,
    ]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(first["status"], "PASS");
    let replay = run(&[
        "governance",
        "check",
        "--snapshot",
        exported,
        "--repository",
        first["repository_id"].as_str().unwrap(),
        "--policies",
        "policies/architecture/domain-boundary",
        "--store",
        store,
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
    let mut legacy: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(exported).unwrap()).unwrap();
    let mut snapshot = legacy["snapshot"].take();
    snapshot.as_object_mut().unwrap().remove("source_coverage");
    let old = dir.join("legacy.json");
    fs::write(&old, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let old_model: nexora_storage::Snapshot = serde_json::from_value(snapshot).unwrap();
    assert!(old_model.source_coverage.is_empty());
    let old_result = run(&[
        "governance",
        "check",
        "--snapshot",
        old.to_str().unwrap(),
        "--repository",
        first["repository_id"].as_str().unwrap(),
        "--policies",
        "policies/architecture/domain-boundary",
        "--store",
        store,
    ]);
    assert_eq!(old_result.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&old_result.stdout).unwrap()["status"],
        "UNKNOWN"
    );
    for (case, status) in [("fail", "FAIL"), ("external", "UNKNOWN"), ("mixed", "FAIL")] {
        let path = format!("policies/architecture/domain-boundary/fixtures/{case}");
        let checked = run(&[
            "governance",
            "check",
            &path,
            "--policies",
            "policies/architecture/domain-boundary",
            "--store",
            store,
        ]);
        assert_eq!(checked.status.code(), Some(1));
        let result: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
        assert_eq!(result["status"], status);
    }
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn legacy_ast_and_cst_nodes_default_to_declarations() {
    let output = run(&["analyze", "tests/fixtures/javascript/declarations.js"]);
    assert_eq!(output.status.code(), Some(0));
    let mut value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for tree in ["cst", "ast"] {
        for node in value[tree]["nodes"].as_array_mut().unwrap() {
            node.as_object_mut().unwrap().remove("role");
        }
    }
    let legacy: nexora_indexer::AnalysisResult = serde_json::from_value(value).unwrap();
    assert!(
        legacy
            .ast
            .nodes
            .iter()
            .all(|n| n.role == nexora_core::SyntaxRole::Declaration)
    );
}
