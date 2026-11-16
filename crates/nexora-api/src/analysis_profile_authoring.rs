use crate::mcp_analysis_profile_package::McpAnalysisProfilePackage;
use nexora_indexer::AnalysisProfileCompiler;
use nexora_language::AnalysisProfilePackage;
use nexora_platform::PlatformError;
use serde_json::{Value, json};

pub(crate) struct AnalysisProfileAuthoring;
impl AnalysisProfileAuthoring {
    pub const MAX_BYTES: usize = 262_144;
    pub fn validate(value: Value) -> Value {
        let package: AnalysisProfilePackage = match serde_json::from_value(value) {
            Ok(package) => package,
            Err(error) => return Self::invalid(error),
        };
        match AnalysisProfileCompiler::compile(&package) {
            Err(error) => Self::invalid(error),
            Ok(_) => match package.checksum() {
                Ok(checksum) => {
                    json!({"valid":true,"sha256":checksum,"language_id":package.language_id,"model_count":package.models.len(),"semantic_coverage_verified":false})
                }
                Err(error) => Self::invalid(error),
            },
        }
    }
    pub fn package(input: McpAnalysisProfilePackage) -> Result<Value, PlatformError> {
        let package = AnalysisProfilePackage {
            format: "nexora.analysis-profile".into(),
            schema_version: 1,
            key: input.key,
            version: input.version,
            name: input.name,
            language_id: input.language_id,
            models: input.models,
        }
        .canonical();
        if let Err(error) = AnalysisProfileCompiler::compile(&package) {
            return Ok(Self::invalid(error));
        }
        let checksum = package.checksum().map_err(|_| PlatformError::Storage)?;
        let text = serde_json::to_string(&package).map_err(|_| PlatformError::Storage)?;
        Ok(
            json!({"valid":true,"file_name":format!("{}-{}.nexora-profile.json",package.key,package.version),"sha256":checksum,"json":text,"semantic_coverage_verified":false}),
        )
    }
    fn invalid(error: impl std::fmt::Display) -> Value {
        let message = error.to_string().chars().take(2000).collect::<String>();
        json!({"valid":false,"errors":[message],"semantic_coverage_verified":false})
    }
}

#[cfg(test)]
#[path = "analysis_profile_authoring_tests.rs"]
mod tests;
