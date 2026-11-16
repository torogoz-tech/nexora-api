use crate::ReasoningError;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonQuery {
    path: String,
    start_line: usize,
    line_count: usize,
}
impl ComparisonQuery {
    pub fn parse(input: Value) -> Result<Self, ReasoningError> {
        let query: Self =
            serde_json::from_value(input).map_err(|_| ReasoningError::InvalidInput)?;
        if query.path.is_empty()
            || query.path.len() > 4096
            || query.path.contains('\0')
            || query.start_line == 0
            || query.start_line > 1000000
            || query.line_count == 0
            || query.line_count > 80
        {
            return Err(ReasoningError::InvalidInput);
        }
        Ok(query)
    }
    pub fn context_arguments(&self) -> Value {
        json!({"path":self.path,"start_line":self.start_line,"line_count":self.line_count})
    }
}
