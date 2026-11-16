use crate::mcp_output::McpOutput;
use serde_json::{Value, json};

#[test]
fn final_serialized_output_is_bounded_and_legacy_version_omits_structured_content() {
    let path = std::env::temp_dir().join(format!(
        "nexora-mcp-output-{}-{}.json",
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
    output
        .tool(
            json!("large"),
            Ok(json!({"value":"\\\"".repeat(400_000)})),
            true,
        )
        .unwrap();
    output
        .tool(json!(1), Ok(json!({"value":"legacy"})), false)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let lines: Vec<_> = bytes
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l.len() <= 1_048_576));
    let large: Value = serde_json::from_slice(lines[0]).unwrap();
    assert_eq!(large["id"], "large");
    assert_eq!(large["result"]["isError"], true);
    let legacy: Value = serde_json::from_slice(lines[1]).unwrap();
    assert!(legacy["result"].get("structuredContent").is_none());
    assert_eq!(legacy["result"]["isError"], false);
}
