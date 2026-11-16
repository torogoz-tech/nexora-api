use crate::api_error::ApiError;
use serde_json::{Value, json};
pub(crate) struct McpToolResult;
impl McpToolResult {
    pub fn output(
        result: Result<Value, ApiError>,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        Ok(match result {
            Ok(value) => {
                let text = serde_json::to_string(&value)
                    .map_err(|_| (-32603, "Result encoding failed"))?;
                let mut output = json!({"isError":false,"content":[{"type":"text","text":text}]});
                if structured {
                    output["structuredContent"] = value;
                }
                output
            }
            Err(ApiError(error)) => {
                json!({"isError":true,"content":[{"type":"text","text":error.to_string()}]})
            }
        })
    }
}
