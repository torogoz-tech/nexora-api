use serde_json::{Value, json};
/// A bounded comparison of two caller-selected, independently authorized snapshots.
pub struct ComparisonTool;
impl ComparisonTool {
    pub const NAME: &str = "nexora_compare_source";
    pub fn definition() -> Value {
        json!({"type":"function","name":Self::NAME,"description":"Read the same source window from the fixed baseline and current authorized snapshots. Compare selected changes; these windows do not establish complete-file coverage, test execution or author identity. Request only indexed paths present in both snapshots.","strict":true,"parameters":{"type":"object","additionalProperties":false,"required":["path","start_line","line_count"],"properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"start_line":{"type":"integer","minimum":1,"maximum":1000000},"line_count":{"type":"integer","minimum":1,"maximum":80}}}})
    }
}
