use std::process::Command;
fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nexora"));
    command.current_dir("../..");
    command
}
#[test]
fn corpus_json_success_and_configuration_failure_exit_codes() {
    let result = command()
        .args(["corpus", "validate", "corpus/suite.yaml"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let description: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(description["cases"].as_array().unwrap().len(), 93);
    let id = description["cases"][0]["id"].as_str().unwrap();
    let result = command()
        .args(["corpus", "check", "corpus/suite.yaml", "--case", id])
        .output()
        .unwrap();
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 1);
    for args in [
        vec!["corpus", "check", "corpus/suite.yaml", "--case", "unknown"],
        vec!["corpus", "validate", "../outside.yaml"],
    ] {
        let result = command().args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
    }
}
#[test]
fn wrong_annotated_answer_exits_one_with_machine_readable_report() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-corpus-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("pack")).unwrap();
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["language.yaml", "lexer.yaml", "grammar.yaml", "graph.yaml"] {
        std::fs::copy(
            workspace.join("templates/language-pack").join(name),
            root.join("pack").join(name),
        )
        .unwrap();
    }
    std::fs::write(root.join("source.nxexample"), "DEF correct [] BEGIN FINISH").unwrap();
    std::fs::write(root.join("suite.yaml"),r#"{"schema_version":1,"id":"test","version":"1.0.0","packs":[{"id":"template","path":"pack"}],"features":[{"id":"test","description":"test"}],"cases":["case.yaml"]}"#).unwrap();
    std::fs::write(root.join("case.yaml"),r#"{"id":"test","pack":"template","feature":"test","classification":"supported","mode":"file","input":"source.nxexample","expected":{"analysis":"clean","facts":[{"type":"declaration","file":"source.nxexample","kind":"Function","name":"wrong","source":"DEF correct [] BEGIN FINISH"}]}}"#).unwrap();
    let result = command()
        .args(["corpus", "check", "suite.yaml", "--workspace"])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["scores"]["false_negatives"], 1);
    assert_eq!(report["scores"]["false_positives"], 1);
    std::fs::remove_dir_all(root).unwrap();
}
