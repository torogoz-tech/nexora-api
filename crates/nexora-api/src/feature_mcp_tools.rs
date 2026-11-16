use crate::{
    ApiState, mcp_feature_proposal::McpFeatureProposal, mcp_feature_query::McpFeatureQuery,
    mcp_tool_result::McpToolResult,
};
use nexora_platform::AuthContext;
use serde_json::{Value, json};
pub(crate) struct FeatureMcpTools;
impl FeatureMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid feature map arguments");
        let result = match name {
            "nexora_feature_complete" => {
                let input: crate::mcp_feature_complete::McpFeatureComplete =
                    serde_json::from_value(args).map_err(invalid)?;
                let completion = nexora_platform::FeatureComplete {
                    expected_revision: input.expected_revision,
                };
                let canonical = json!({"project_id":input.project_id,"feature_id":input.feature_id,"completion":completion});
                state
                    .mutate(
                        ctx,
                        "feature.complete",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.complete_feature(
                                ctx,
                                &input.project_id,
                                &input.feature_id,
                                &completion,
                            )
                        },
                    )
                    .await
            }
            "nexora_features_list" => {
                let input: McpFeatureQuery = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| store.feature_catalog(&ctx, &input.project_id))
                    .await
            }
            "nexora_feature_map_propose" => {
                let input: McpFeatureProposal = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"proposal":input.proposal});
                state
                    .mutate(
                        ctx,
                        "feature_map.propose",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.propose_feature_map(ctx, &input.project_id, &input.proposal)
                        },
                    )
                    .await
            }
            _ => return Err((-32602, "Unknown feature tool")),
        };
        McpToolResult::output(result, structured)
    }
}
