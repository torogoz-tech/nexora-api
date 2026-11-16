use crate::code_mcp_query::CodeMcpQuery;
use nexora_platform::{AuthContext, PlatformError};
use nexora_platform_store::PlatformStore;
use serde_json::{Value, json};
/// Reads through the same live authorization boundary as ordinary MCP code queries.
pub(crate) struct ReasoningRepository {
    pub store: PlatformStore,
    pub ctx: AuthContext,
    pub project: String,
    pub repository: String,
    pub snapshot: Option<String>,
    pub identity: Option<Value>,
}
impl ReasoningRepository {
    pub fn read(&mut self, name: &str, args: Value) -> Result<Value, PlatformError> {
        if !CodeMcpQuery::supports(name) {
            return Err(PlatformError::Forbidden);
        }
        let mut args = args.as_object().cloned().ok_or(PlatformError::Validation(
            "invalid reasoning tool arguments",
        ))?;
        if ["project_id", "repository_id", "snapshot"]
            .iter()
            .any(|k| args.contains_key(*k))
        {
            return Err(PlatformError::Forbidden);
        }
        args.insert("project_id".into(), json!(self.project));
        args.insert("repository_id".into(), json!(self.repository));
        if let Some(snapshot) = &self.snapshot {
            args.insert("snapshot".into(), json!(snapshot));
        }
        let input = CodeMcpQuery::parse(name, Value::Object(args))
            .map_err(|_| PlatformError::Validation("invalid reasoning tool arguments"))?;
        let result = self.store.query_repository_code(
            &self.ctx,
            &input.project_id,
            &input.repository_id,
            input.snapshot.as_deref(),
            &input.query,
        )?;
        let snapshot = result["snapshot"].as_str().ok_or(PlatformError::Storage)?;
        let identity = json!({"project_id":self.project,"repository_id":self.repository,"snapshot":snapshot,"commit":result["commit"],"local_origin":result["local_origin"]});
        if self.snapshot.as_ref().is_some_and(|s| s != snapshot)
            || self.identity.as_ref().is_some_and(|i| i != &identity)
        {
            return Err(PlatformError::Conflict("reasoning snapshot changed"));
        }
        self.snapshot = Some(snapshot.into());
        self.identity = Some(identity);
        Ok(result)
    }
    pub fn compare_with(
        &mut self,
        baseline: &mut Self,
        args: Value,
        budget: std::time::Duration,
    ) -> Result<Value, PlatformError> {
        if self.project != baseline.project
            || self.repository != baseline.repository
            || self.snapshot.is_none()
            || baseline.snapshot.is_none()
            || self.snapshot == baseline.snapshot
        {
            return Err(PlatformError::Validation(
                "comparison requires distinct snapshots in the same repository",
            ));
        }
        let query = nexora_reasoning::ComparisonQuery::parse(args)
            .map_err(|_| PlatformError::Validation("invalid comparison arguments"))?;
        let started = std::time::Instant::now();
        let before = baseline.read("nexora_get_source_context", query.context_arguments())?;
        if started.elapsed() >= budget {
            return Err(PlatformError::Unavailable);
        }
        let after = self.read("nexora_get_source_context", query.context_arguments())?;
        if started.elapsed() >= budget {
            return Err(PlatformError::Unavailable);
        }
        Ok(
            json!({"comparison":"selected_source_windows","before":before,"after":after,"authorship":"not_independently_verified"}),
        )
    }
}
