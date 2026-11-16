use std::process::Command;
fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nexora"));
    cmd.current_dir("../..");
    cmd
}
#[test]
fn version_validation_and_demo_are_real() {
    let output = command().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("0.1.0"));
    assert!(
        command()
            .args(["languages", "validate", "languages/javascript"])
            .status()
            .unwrap()
            .success()
    );
    let output = command()
        .args(["analyze", "tests/fixtures/javascript/declarations.js"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ast"]["nodes"].as_array().unwrap().len(), 2);
    let output = command()
        .args(["index", "tests/fixtures/javascript", "--offline"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["sources"].as_object().unwrap().len(), 1);
}
#[test]
fn missing_source_has_failure_exit_status() {
    assert!(
        !command()
            .args(["analyze", "missing.js"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn markup_catalog_discovers_header_dependencies_and_emits_proven_interpretations() {
    let output = command()
        .args([
            "--language-dir",
            "templates/composition-markup",
            "analyze",
            "templates/composition-markup/fixtures/complete/source.nxb",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["diagnostics"].as_array().unwrap().is_empty());
    assert!(
        value["embedded_units"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["context"] == "attributes")
    );
    assert!(
        value["graph"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "INTERPRETS" && e["label"] == "header")
    );
    assert!(
        value["graph"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "count")
    );
    for coverage in value["graph"]["syntax_coverage"]
        .as_object()
        .unwrap()
        .values()
    {
        assert_eq!(coverage["opaque_regions"], 0);
        assert_eq!(coverage["partial_regions"], 0);
    }
}

#[test]
fn scoped_catalog_cli_emits_host_bindings_with_original_reference_sites() {
    let output = command()
        .args([
            "--language-dir",
            "templates/composition-scopes",
            "analyze",
            "templates/composition-scopes/fixtures/complete/source.nxb",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["diagnostics"].as_array().unwrap().is_empty());
    let edges = value["graph"]["edges"].as_array().unwrap();
    assert!(edges.iter().any(|e| e["kind"] == "SCOPE_BRIDGE"));
    let references: Vec<_> = edges.iter().filter(|e| e["kind"] == "REFERENCES").collect();
    assert_eq!(references.len(), 4);
    assert!(
        references
            .iter()
            .all(|e| e["resolution"] == "Resolved" && e["scope_binding"]["site"].is_string())
    );
}

#[test]
fn direct_call_catalog_cli_emits_proven_foreign_calls() {
    let output = command()
        .args([
            "--language-dir",
            "templates/composition-calls",
            "analyze",
            "templates/composition-calls/fixtures/complete/source.nxb",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["diagnostics"].as_array().unwrap().is_empty());
    let calls: Vec<_> = value["graph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "CALLS")
        .collect();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|e| e["resolution"] == "Resolved" && e["call_binding"]["reference"].is_string())
    );
}

#[test]
fn web_understanding_catalog_cli_reads_real_nested_bodies_without_claiming_type_completeness() {
    let output = command()
        .args([
            "--language-dir",
            "templates/web-understanding",
            "analyze",
            "templates/web-understanding/fixtures/body/source.ts",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nodes = value["graph"]["nodes"].as_array().unwrap();
    for name in ["compute", "helper", "local", "nested"] {
        assert!(nodes.iter().any(|n| n["name"] == name));
    }
    assert!(nodes.iter().any(|n| n["expression"]["operator"] == "*"));
    assert_eq!(value["symbols"]["complete"], false);
}
