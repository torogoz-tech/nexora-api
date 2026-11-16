use crate::{
    ApiState, analysis_profile_evolution_input::AnalysisProfileEvolutionInput,
    mcp_tool_result::McpToolResult,
};
use nexora_platform::AuthContext;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct AnalysisProfileEvolutionMcp;
impl AnalysisProfileEvolutionMcp {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!(
            "../schemas/analysis_profile_evolution_tools.json"
        ))
        .expect("profile evolution tools")
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        if serde_json::to_vec(&args)
            .map_err(|_| (-32602, "Invalid arguments"))?
            .len()
            > 262_144
        {
            return Err((-32602, "Profile request exceeds 256 KiB"));
        }
        let input: AnalysisProfileEvolutionInput =
            serde_json::from_value(args).map_err(|_| (-32602, "Invalid profile arguments"))?;
        let result = match name {
            "nexora_analysis_profiles_list" => {
                state
                    .run(move |store| {
                        store.analysis_profiles(
                            &ctx,
                            input.after.as_deref(),
                            input.limit.unwrap_or(20),
                        )
                    })
                    .await
            }
            "nexora_analysis_profile_get" => {
                let id = input.profile_id.ok_or((-32602, "profile_id required"))?;
                state
                    .run(move |store| store.analysis_profile(&ctx, &id, true))
                    .await
            }
            "nexora_analysis_profile_install" => {
                let package = input.package.ok_or((-32602, "package required"))?;
                let sha = format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&package.canonical())
                            .map_err(|_| (-32602, "Invalid package"))?
                    )
                );
                state
                    .mutate(
                        ctx,
                        "analysis_profile.import",
                        input.request_id.ok_or((-32602, "request_id required"))?,
                        json!({"package_sha256":sha}),
                        move |store, ctx| store.import_analysis_profile(ctx, &package),
                    )
                    .await
            }
            "nexora_repository_analysis_issues" => {
                let project = input.project_id.ok_or((-32602, "project_id required"))?;
                let repo = input
                    .repository_id
                    .ok_or((-32602, "repository_id required"))?;
                state
                    .run(move |store| {
                        store.repository_analysis_issues(
                            &ctx,
                            &project,
                            &repo,
                            input.offset.unwrap_or(0),
                            input.limit.unwrap_or(50),
                        )
                    })
                    .await
            }
            _ => return Err((-32601, "Unknown profile tool")),
        };
        let result = McpToolResult::output(result, structured)?;
        if serde_json::to_vec(&result)
            .map_err(|_| (-32603, "Encoding failed"))?
            .len()
            > 1_000_000
        {
            return Err((-32602, "Profile result exceeds MCP budget"));
        }
        Ok(result)
    }
}
