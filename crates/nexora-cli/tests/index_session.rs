use std::process::Command;
fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nexora"));
    cmd.current_dir("../..");
    cmd
}
#[test]
fn session_reports_cold_and_warm_reuse_without_changing_snapshot() {
    let result = command()
        .args([
            "index-session",
            "corpus/fixtures/template/scaling",
            "--language-dir",
            "templates/language-pack",
            "--passes",
            "3",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let passes = report["passes"].as_array().unwrap();
    assert_eq!(passes.len(), 3);
    assert_eq!(passes[0]["reuse"]["analyzed_files"], 2);
    assert_eq!(passes[0]["reuse"]["reused_files"], 0);
    assert_eq!(passes[1]["reuse"]["analyzed_files"], 0);
    assert_eq!(passes[1]["reuse"]["reused_files"], 2);
    assert_eq!(passes[2]["snapshot_id"], passes[0]["snapshot_id"]);
    assert_eq!(
        report["final_snapshot"]["sources"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn session_capacity_zero_is_correct_and_pass_count_is_bounded() {
    let result = command()
        .args([
            "index-session",
            "corpus/fixtures/template/scaling",
            "--language-dir",
            "templates/language-pack",
            "--cache-bytes",
            "0",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passes"][1]["reuse"]["reused_files"], 0);
    assert_eq!(report["passes"][1]["reuse"]["analyzed_files"], 2);
    for count in ["0", "1", "101"] {
        assert_eq!(
            command()
                .args([
                    "index-session",
                    "corpus/fixtures/template/scaling",
                    "--passes",
                    count
                ])
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
}
