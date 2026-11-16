use crate::mcp_analysis_model_guide_get::McpAnalysisModelGuideGet;
use nexora_platform::PlatformError;
use serde_json::{Value, json};
pub(crate) struct AnalysisModelGuide;
impl AnalysisModelGuide {
    pub fn get(input: McpAnalysisModelGuideGet) -> Result<Value, PlatformError> {
        let Some(component) = input.component.as_deref() else {
            if input.version.is_some() {
                return Err(PlatformError::Validation(
                    "component required with schema version",
                ));
            }
            return Ok(
                json!({"format":"nexora.analysis-profile","schema_version":1,"max_request_bytes":262144,
                "components":[{"name":"language","versions":[1]},{"name":"lexer","versions":(1..=14).collect::<Vec<_>>()},{"name":"grammar","versions":(2..=10).collect::<Vec<_>>()},{"name":"graph","versions":(1..=10).collect::<Vec<_>>()}],
                "workflow":["Read the component schemas and author declarative rules with your development agent.","Include exactly the root language and its transitive composition dependencies.","Call nexora_analysis_profile_validate and correct reported errors.","Call nexora_analysis_profile_package, save its json string as file_name and share the file.","Call nexora_analysis_profile_install to validate and install a new immutable version. Automatic repositories use the latest compatible version on their next synchronization; manual assignments remain pinned."],
                "constraints":["Models are data, never executable code or server paths.","Validation compiles the rules; it does not certify semantic coverage. Test your model against a representative corpus.","These tools never install models, change assignments, read server source files or call an AI provider."]}),
            );
        };
        let latest = match component {
            "language" => 1,
            "lexer" => 14,
            "grammar" | "graph" => 10,
            _ => return Err(PlatformError::Validation("unknown model component")),
        };
        let version = input.version.unwrap_or(latest);
        let text = match (component, version) {
            ("language", 1) => include_str!("../../../schemas/language-v1.schema.json"),
            ("lexer", 1) => include_str!("../../../schemas/lexer-v1.schema.json"),
            ("lexer", 2) => include_str!("../../../schemas/lexer-v2.schema.json"),
            ("lexer", 3) => include_str!("../../../schemas/lexer-v3.schema.json"),
            ("lexer", 4) => include_str!("../../../schemas/lexer-v4.schema.json"),
            ("lexer", 5) => include_str!("../../../schemas/lexer-v5.schema.json"),
            ("lexer", 6) => include_str!("../../../schemas/lexer-v6.schema.json"),
            ("lexer", 7) => include_str!("../../../schemas/lexer-v7.schema.json"),
            ("lexer", 8) => include_str!("../../../schemas/lexer-v8.schema.json"),
            ("lexer", 9) => include_str!("../../../schemas/lexer-v9.schema.json"),
            ("lexer", 10) => include_str!("../../../schemas/lexer-v10.schema.json"),
            ("lexer", 11) => include_str!("../../../schemas/lexer-v11.schema.json"),
            ("lexer", 12) => include_str!("../../../schemas/lexer-v12.schema.json"),
            ("lexer", 13) => include_str!("../../../schemas/lexer-v13.schema.json"),
            ("lexer", 14) => include_str!("../../../schemas/lexer-v14.schema.json"),
            ("grammar", 2) => include_str!("../../../schemas/grammar-v2.schema.json"),
            ("grammar", 3) => include_str!("../../../schemas/grammar-v3.schema.json"),
            ("grammar", 4) => include_str!("../../../schemas/grammar-v4.schema.json"),
            ("grammar", 5) => include_str!("../../../schemas/grammar-v5.schema.json"),
            ("grammar", 6) => include_str!("../../../schemas/grammar-v6.schema.json"),
            ("grammar", 7) => include_str!("../../../schemas/grammar-v7.schema.json"),
            ("grammar", 8) => include_str!("../../../schemas/grammar-v8.schema.json"),
            ("grammar", 9) => include_str!("../../../schemas/grammar-v9.schema.json"),
            ("grammar", 10) => include_str!("../../../schemas/grammar-v10.schema.json"),
            ("graph", 1) => include_str!("../../../schemas/graph-v1.schema.json"),
            ("graph", 2) => include_str!("../../../schemas/graph-v2.schema.json"),
            ("graph", 3) => include_str!("../../../schemas/graph-v3.schema.json"),
            ("graph", 4) => include_str!("../../../schemas/graph-v4.schema.json"),
            ("graph", 5) => include_str!("../../../schemas/graph-v5.schema.json"),
            ("graph", 6) => include_str!("../../../schemas/graph-v6.schema.json"),
            ("graph", 7) => include_str!("../../../schemas/graph-v7.schema.json"),
            ("graph", 8) => include_str!("../../../schemas/graph-v8.schema.json"),
            ("graph", 9) => include_str!("../../../schemas/graph-v9.schema.json"),
            ("graph", 10) => include_str!("../../../schemas/graph-v10.schema.json"),
            _ => {
                return Err(PlatformError::Validation(
                    "unsupported model schema version",
                ));
            }
        };
        let schema: Value = serde_json::from_str(text).map_err(|_| PlatformError::Storage)?;
        Ok(json!({"component":component,"version":version,"schema":schema}))
    }
}
