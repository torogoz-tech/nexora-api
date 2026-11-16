use crate::{
    ApiState, analysis_model_guide::AnalysisModelGuide,
    analysis_profile_authoring::AnalysisProfileAuthoring, api_error::ApiError,
    mcp_analysis_model_guide_get::McpAnalysisModelGuideGet,
    mcp_analysis_profile_package::McpAnalysisProfilePackage,
    mcp_analysis_profile_validate::McpAnalysisProfileValidate, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct AnalysisProfileAuthoringMcpTools;
impl AnalysisProfileAuthoringMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let operation = name.to_owned();
        let result = state
            .run(move |store| {
                // No project or administrative data is read. Revalidate the live
                // credential before spending compilation resources on caller-owned input.
                store.effective_permissions(&ctx)?;
                if serde_json::to_vec(&args)
                    .map_err(|_| PlatformError::Storage)?
                    .len()
                    > AnalysisProfileAuthoring::MAX_BYTES
                {
                    return Err(PlatformError::Validation(
                        "model authoring request exceeds 256 KiB",
                    ));
                }
                match operation.as_str() {
                    "nexora_analysis_model_guide_get" => {
                        let input: McpAnalysisModelGuideGet = serde_json::from_value(args)
                            .map_err(|_| {
                                PlatformError::Validation("invalid model guide arguments")
                            })?;
                        AnalysisModelGuide::get(input)
                    }
                    "nexora_analysis_profile_validate" => {
                        let input: McpAnalysisProfileValidate = serde_json::from_value(args)
                            .map_err(|_| {
                                PlatformError::Validation("invalid profile validation arguments")
                            })?;
                        Ok(AnalysisProfileAuthoring::validate(input.package))
                    }
                    "nexora_analysis_profile_package" => {
                        let input: McpAnalysisProfilePackage = serde_json::from_value(args)
                            .map_err(|_| {
                                PlatformError::Validation("invalid profile packaging arguments")
                            })?;
                        AnalysisProfileAuthoring::package(input)
                    }
                    _ => Err(PlatformError::Validation("unknown authoring operation")),
                }
            })
            .await;
        let output = McpToolResult::output(result, structured)?;
        // Check the actual envelope, including duplicated structured/text content.
        // No persisted side effects have occurred if it cannot fit the transport.
        if serde_json::to_vec(&output)
            .map_err(|_| (-32603, "Result encoding failed"))?
            .len()
            > 1_000_000
        {
            return McpToolResult::output(
                Err(ApiError(PlatformError::Validation(
                    "packaged model exceeds MCP response budget",
                ))),
                structured,
            );
        }
        Ok(output)
    }
}
