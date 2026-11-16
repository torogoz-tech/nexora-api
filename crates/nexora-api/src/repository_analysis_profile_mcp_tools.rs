use crate::{
    ApiState, api_error::ApiError,
    mcp_repository_analysis_profile_get::McpRepositoryAnalysisProfileGet,
    mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;

pub(crate) struct RepositoryAnalysisProfileMcpTools;
impl RepositoryAnalysisProfileMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let input: McpRepositoryAnalysisProfileGet = serde_json::from_value(args)
            .map_err(|_| (-32602, "Invalid repository profile lookup"))?;
        let result = state
            .run(move |store| {
                store.repository_analysis_profile(&ctx, &input.project_id, &input.repository_id)
            })
            .await;
        Self::output(result, structured)
    }
    fn output(
        result: Result<Value, ApiError>,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let output = McpToolResult::output(result, structured)?;
        // Reports may contain up to the full repository inventory. Check the
        // actual duplicated MCP envelope; REST remains the full report route.
        if serde_json::to_vec(&output)
            .map_err(|_| (-32603, "Result encoding failed"))?
            .len()
            > 1_000_000
        {
            return McpToolResult::output(
                Err(ApiError(PlatformError::Validation(
                    "analysis selection report exceeds MCP budget; read the repository analysis-profile REST endpoint",
                ))),
                structured,
            );
        }
        Ok(output)
    }
}

#[cfg(test)]
#[path = "repository_analysis_profile_mcp_tests.rs"]
mod tests;
