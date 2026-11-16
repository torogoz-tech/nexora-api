use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires prepared local PostgreSQL, Dragonfly and Neo4j; run explicitly with --ignored"]
fn separate_cli_processes_persist_read_and_reuse_without_loading_molds_for_queries() {
    let root = Path::new("../..").canonicalize().unwrap();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-cli-databases-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "function hello() {}\n").unwrap();
    fs::write(dir.join("b.js"), "class World {}\n").unwrap();
    let tenant = format!("public-cli-{unique}");
    let run = |args: &[&str], models: bool| -> Value {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nexora"));
        command
            .current_dir(&dir)
            .arg("--connections")
            .arg(root.join(".nexora/local/connections.json"));
        command.arg("--language-dir").arg(if models {
            root.join("languages/javascript")
        } else {
            dir.join("does-not-exist")
        });
        let output = command.args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let first = run(
        &[
            "index",
            dir.to_str().unwrap(),
            "--tenant",
            &tenant,
            "--report",
        ],
        true,
    );
    assert_eq!(first["reuse"]["analyzed_files"], 2);
    let second = run(
        &[
            "index",
            dir.to_str().unwrap(),
            "--tenant",
            &tenant,
            "--report",
        ],
        true,
    );
    assert_eq!(second["reuse"]["reused_files"], 2);
    assert_eq!(second["reuse"]["analyzed_files"], 0);
    assert_eq!(first["snapshot"], second["snapshot"]);
    let repository = first["snapshot"]["repository_id"].as_str().unwrap();
    let snapshot = first["snapshot"]["id"].as_str().unwrap();
    let restored = run(
        &[
            "snapshot",
            "get",
            "--tenant",
            &tenant,
            "--repository",
            repository,
        ],
        false,
    );
    assert_eq!(restored, first["snapshot"]);
    let node = restored["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "File")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let graph = run(
        &[
            "graph",
            "traverse",
            "--tenant",
            &tenant,
            "--repository",
            repository,
            "--snapshot",
            snapshot,
            "--node",
            node,
        ],
        false,
    );
    assert!(!graph["edge_ids"].as_array().unwrap().is_empty());
    fs::remove_dir_all(dir).unwrap();
}
