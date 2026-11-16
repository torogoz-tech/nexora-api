use nexora_mcp::{McpConfiguration, StdioServer};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn run(lines: Vec<Value>) -> Vec<Value> {
    let input = lines
        .into_iter()
        .map(|v| v.to_string() + "\n")
        .collect::<String>();
    raw(input.as_bytes())
}
fn raw(input: &[u8]) -> Vec<Value> {
    let capture = Capture(Arc::new(Mutex::new(Vec::new())));
    let config = McpConfiguration::new(
        Path::new("../../tests/fixtures/javascript"),
        "mcp-protocol",
        Path::new("missing-connections"),
        Path::new("missing-molds"),
        None,
        false,
    )
    .unwrap();
    StdioServer::serve(config, std::io::Cursor::new(input), capture.clone()).unwrap();
    let bytes = capture.0.lock().unwrap();
    std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn initialize(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"public-test","version":"1"}}})
}

#[test]
fn lifecycle_negotiation_and_discovery_need_no_database_or_molds() {
    let replies = run(vec![
        json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}),
        initialize("2025-11-25"),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":3,"method":"resources/list"}),
    ]);
    assert_eq!(replies.len(), 5);
    assert!(replies[0]["error"].is_object());
    assert_eq!(replies[1]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(replies[2]["result"]["tools"].as_array().unwrap().len(), 10);
    assert_eq!(replies[3]["result"], json!({}));
    assert_eq!(replies[4]["error"]["code"], -32601);
}
#[test]
fn malformed_json_oversized_lines_and_invalid_ids_do_not_break_the_next_request() {
    let mut input = String::from("{broken\n");
    input.push_str(&"x".repeat(1_048_580));
    input.push('\n');
    input.push_str("{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}\n");
    input.push_str("{\"jsonrpc\":\"2.0\",\"id\":true,\"method\":\"ping\"}\n");
    input.push_str("{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n");
    let replies = raw(input.as_bytes());
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[4]["id"], 7);
    assert_eq!(replies[4]["result"], json!({}));
}
#[test]
fn tool_arguments_unknown_tools_and_notifications_follow_jsonrpc_contract() {
    let replies = run(vec![
        initialize("2025-06-18"),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","method":"tools/list"}),
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"unknown"}}),
        json!({"jsonrpc":"2.0","id":"bad","method":"tools/call","params":{"name":"nexora_find_symbol","arguments":{"name":"x","tenant":"other"}}}),
        json!({"jsonrpc":"2.0","id":"disabled","method":"tools/call","params":{"name":"nexora_index_repository","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":"repeat","method":"initialize","params":{}}),
    ]);
    assert_eq!(replies.len(), 4);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[1]["error"]["code"], -32602);
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert!(replies[3]["error"].is_object());
}
#[test]
fn invalid_utf8_batch_and_bad_initialization_are_rejected_cleanly() {
    let replies = raw(b"\xff\n[]\n{\"jsonrpc\":\"1.0\",\"id\":3,\"method\":\"ping\"}\n");
    assert_eq!(replies.len(), 3);
    assert!(replies.iter().all(|r| r["error"].is_object()));
    let bad = run(vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
    ]);
    assert!(bad[0]["error"].is_object());
}

#[test]
fn scope_rejects_file_roots_and_symlink_roots() {
    assert!(
        McpConfiguration::new(
            Path::new("../../tests/fixtures/javascript/declarations.js"),
            "test",
            Path::new("missing"),
            Path::new("missing"),
            None,
            false
        )
        .is_err()
    );
    assert!(
        McpConfiguration::new(
            Path::new("../../tests/fixtures/javascript"),
            "",
            Path::new("missing"),
            Path::new("missing"),
            None,
            false
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let path = std::env::temp_dir().join(format!("nexora-mcp-link-{}", std::process::id()));
        std::os::unix::fs::symlink(
            Path::new("../../tests/fixtures/javascript")
                .canonicalize()
                .unwrap(),
            &path,
        )
        .unwrap();
        let rejected = McpConfiguration::new(
            &path,
            "test",
            Path::new("missing"),
            Path::new("missing"),
            None,
            false,
        )
        .is_err();
        std::fs::remove_file(path).unwrap();
        assert!(rejected);
    }
}
