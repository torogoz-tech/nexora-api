use crate::{
    mcp_output::McpOutput, protocol_session::ProtocolSession, request_registry::RequestRegistry,
};
use serde_json::json;
use std::sync::{Arc, Mutex, mpsc};

#[test]
fn ping_remains_responsive_when_all_tool_slots_are_occupied() {
    let registry = Arc::new(Mutex::new(RequestRegistry::default()));
    for i in 0..8 {
        registry.lock().unwrap().register(&i.to_string()).unwrap();
    }
    let mut session = ProtocolSession::new(false, registry);
    let path = std::env::temp_dir().join(format!(
        "nexora-mcp-ping-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let output = McpOutput::new(file);
    let (send, _receive) = mpsc::sync_channel(8);
    session
        .handle(
            json!({"jsonrpc":"2.0","id":"ping","method":"ping"}),
            &output,
            &send,
        )
        .unwrap();
    let response: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(response["result"], json!({}), "{response}");
}

#[test]
fn cancellation_and_typed_ids_preserve_other_inflight_requests() {
    let mut registry = RequestRegistry::default();
    registry.register("1").unwrap();
    registry.register("\"1\"").unwrap();
    assert!(registry.register("1").is_err());
    registry.cancel("unknown");
    registry.cancel("1");
    assert!(registry.cancelled("1"));
    assert!(!registry.cancelled("\"1\""));
    assert!(registry.complete("1"));
    assert!(!registry.complete("\"1\""));
    registry.register("1").unwrap();
}
