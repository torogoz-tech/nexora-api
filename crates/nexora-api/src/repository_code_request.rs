use crate::{code_mcp_query::CodeMcpQuery, code_query_operation::CodeQueryOperation};
use nexora_platform::PlatformError;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositoryCodeRequest {
    pub operation: CodeQueryOperation,
    #[serde(default)]
    pub arguments: Map<String, Value>,
}

impl RepositoryCodeRequest {
    pub fn resolve(
        self,
        project: String,
        repository: String,
    ) -> Result<CodeMcpQuery, PlatformError> {
        let mut arguments = self.arguments;
        if arguments.contains_key("project_id") || arguments.contains_key("repository_id") {
            return Err(PlatformError::Validation(
                "code scope comes from route parameters",
            ));
        }
        arguments.insert("project_id".into(), Value::String(project));
        arguments.insert("repository_id".into(), Value::String(repository));
        CodeMcpQuery::parse(self.operation.tool_name(), Value::Object(arguments))
            .map_err(|_| PlatformError::Validation("invalid code query arguments"))
    }
}
