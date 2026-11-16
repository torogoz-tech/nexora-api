use std::path::Path;
use std::process::{Command, Output};
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nexora"))
        .args([
            "--language-dir",
            Path::new("../../templates/composition-molds")
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap(),
            "--language",
            "region_carrier",
        ])
        .args(args)
        .output()
        .unwrap()
}
fn value(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn analyze_index_session_and_governance_use_declared_composition_catalogs() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-composed-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    let file = source.join("a.nxb");
    std::fs::write(&file, "~[@(1 + 2)]~").unwrap();
    let analysis = value(run(&["analyze", file.to_str().unwrap()]));
    assert_eq!(analysis["embedded_units"].as_array().unwrap().len(), 1);
    let snapshot = value(run(&["index", source.to_str().unwrap(), "--offline"]));
    assert_eq!(
        snapshot["graph"]["syntax_units"].as_object().unwrap().len(),
        2
    );
    let session = value(run(&[
        "index-session",
        source.to_str().unwrap(),
        "--passes",
        "2",
    ]));
    assert_eq!(session["passes"][1]["reuse"]["reused_files"], 1);
    assert_eq!(session["passes"][1]["timings"]["composition_ms"], 0.0);
    let policy = root.join("unit.policy.yaml");
    std::fs::write(&policy,"apiVersion: nexora.dev/v1alpha1\nkind: Policy\nmetadata:\n  id: INTERIOR-EXPRESSIONS\n  name: Interior expressions\n  description: Verify expressions in the embedded language\n  version: 1.0.0\n  severity: low\n  category: maintainability\nspec:\n  target:\n    language: region_expression\n    symbol: expression\n  assert:\n    attribute: name\n    operator: matches\n    value: '.*'\n  onUnknown: review\n").unwrap();
    let export = root.join("snapshot.json");
    let report = value(run(&[
        "governance",
        "check",
        source.to_str().unwrap(),
        "--policies",
        policy.to_str().unwrap(),
        "--store",
        root.join("reports").to_str().unwrap(),
        "--export-snapshot",
        export.to_str().unwrap(),
    ]));
    assert_eq!(report["status"], "PASS");
    let replay = value(run(&[
        "governance",
        "check",
        "--snapshot",
        export.to_str().unwrap(),
        "--repository",
        report["repository_id"].as_str().unwrap(),
        "--policies",
        policy.to_str().unwrap(),
        "--store",
        root.join("replay").to_str().unwrap(),
    ]));
    assert_eq!(replay["id"], report["id"]);
    std::fs::write(root.join("unit.tests.yaml"),"apiVersion: nexora.dev/v1alpha1\nkind: PolicyTestSuite\ncases:\n  - name: composed fixture\n    policy: INTERIOR-EXPRESSIONS\n    source: source\n    expected: PASS\n").unwrap();
    let fixture = value(run(&["policy", "test", root.to_str().unwrap()]));
    assert_eq!(fixture[0]["passed"], true);
    std::fs::remove_dir_all(root).unwrap();
}
